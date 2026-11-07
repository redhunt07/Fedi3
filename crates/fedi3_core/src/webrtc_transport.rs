//! Adapts the current WebRTC driver to Fedi3's existing signaling/session layer.
//! Only the transport API changes; SDP, trickle ICE and binary frames stay standard.
use anyhow::Result;
use bytes::{Bytes, BytesMut};
use std::{
    future::Future,
    pin::Pin,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex, Weak,
    },
};
use tokio::sync::watch;
use webrtc::{
    data_channel::{DataChannel, DataChannelEvent, RTCDataChannelMessage},
    peer_connection::{
        PeerConnection, PeerConnectionBuilder, PeerConnectionEventHandler, RTCConfigurationBuilder,
        RTCIceCandidate, RTCIceCandidateInit, RTCIceGatheringState, RTCIceServer,
        RTCPeerConnectionIceEvent, RTCPeerConnectionState, RTCSessionDescription,
    },
};

pub use webrtc::data_channel::RTCDataChannelMessage as DataChannelMessage;
pub use webrtc::peer_connection::{
    RTCIceCandidateInit as IceCandidateInit, RTCIceServer as IceServer,
    RTCPeerConnectionState as ConnectionState, RTCSessionDescription as SessionDescription,
};

type CallbackFuture = Pin<Box<dyn Future<Output = ()> + Send>>;
type Callback<T> = Box<dyn FnMut(T) -> CallbackFuture + Send + Sync>;
type OpenCallback = Box<dyn FnOnce() -> CallbackFuture + Send + Sync>;

#[derive(Default)]
struct Callbacks {
    candidate: Mutex<Option<Callback<Option<RTCIceCandidate>>>>,
    channel: Mutex<Option<Callback<Arc<Channel>>>>,
    state: Mutex<Option<Callback<RTCPeerConnectionState>>>,
    channels: Mutex<Vec<Weak<Channel>>>,
    gathered: AtomicBool,
    gather_notify: tokio::sync::Notify,
}
impl Callbacks {
    fn clear(&self) {
        self.candidate.lock().unwrap().take();
        self.channel.lock().unwrap().take();
        self.state.lock().unwrap().take();
        for channel in self
            .channels
            .lock()
            .unwrap()
            .drain(..)
            .filter_map(|c| c.upgrade())
        {
            channel.stop.send_replace(true);
            channel.message.lock().unwrap().take();
            channel.open.lock().unwrap().take();
        }
    }
    fn wrap(&self, inner: Arc<dyn DataChannel>) -> Arc<Channel> {
        let channel = Arc::new(Channel {
            inner,
            message: Mutex::new(None),
            open: Mutex::new(None),
            opened: AtomicBool::new(false),
            stop: watch::channel(false).0,
        });
        self.channels.lock().unwrap().push(Arc::downgrade(&channel));
        channel
    }
}
#[async_trait::async_trait]
impl PeerConnectionEventHandler for Callbacks {
    async fn on_ice_gathering_state_change(&self, state: RTCIceGatheringState) {
        if state == RTCIceGatheringState::Complete {
            self.gathered.store(true, Ordering::Release);
            self.gather_notify.notify_waiters();
        }
    }
    async fn on_ice_candidate(&self, event: RTCPeerConnectionIceEvent) {
        let future = self
            .candidate
            .lock()
            .unwrap()
            .as_mut()
            .map(|cb| cb(Some(event.candidate)));
        if let Some(future) = future {
            tokio::spawn(future);
        }
    }
    async fn on_data_channel(&self, inner: Arc<dyn DataChannel>) {
        let channel = self.wrap(inner);
        let future = self
            .channel
            .lock()
            .unwrap()
            .as_mut()
            .map(|cb| cb(channel.clone()));
        if let Some(future) = future {
            future.await;
        }
        channel.start_reader();
    }
    async fn on_connection_state_change(&self, state: RTCPeerConnectionState) {
        let future = self.state.lock().unwrap().as_mut().map(|cb| cb(state));
        if let Some(future) = future {
            tokio::spawn(future);
        }
        if state == RTCPeerConnectionState::Closed {
            self.clear();
        }
    }
}

pub struct Connection {
    inner: Arc<dyn PeerConnection>,
    callbacks: Arc<Callbacks>,
}
impl Connection {
    pub async fn new(ice_servers: Vec<RTCIceServer>) -> Result<Arc<Self>> {
        let callbacks = Arc::new(Callbacks::default());
        let inner = PeerConnectionBuilder::new()
            .with_configuration(
                RTCConfigurationBuilder::default()
                    .with_ice_servers(ice_servers)
                    .build(),
            )
            .with_handler(callbacks.clone())
            .with_udp_addrs(vec!["0.0.0.0:0"])
            .with_data_channel_send_buffer_limit(4 * 1024 * 1024)
            .build()
            .await?;
        Ok(Arc::new(Self {
            inner: Arc::new(inner),
            callbacks,
        }))
    }
    pub fn on_ice_candidate(&self, callback: Callback<Option<RTCIceCandidate>>) {
        *self.callbacks.candidate.lock().unwrap() = Some(callback);
    }
    pub fn on_data_channel(&self, callback: Callback<Arc<Channel>>) {
        *self.callbacks.channel.lock().unwrap() = Some(callback);
    }
    pub fn on_peer_connection_state_change(&self, callback: Callback<RTCPeerConnectionState>) {
        *self.callbacks.state.lock().unwrap() = Some(callback);
    }
    pub async fn create_offer(&self, _: Option<()>) -> Result<RTCSessionDescription> {
        Ok(self.inner.create_offer(None).await?)
    }
    pub async fn create_answer(&self, _: Option<()>) -> Result<RTCSessionDescription> {
        Ok(self.inner.create_answer(None).await?)
    }
    pub async fn set_local_description(&self, description: RTCSessionDescription) -> Result<()> {
        Ok(self.inner.set_local_description(description).await?)
    }
    pub async fn set_remote_description(&self, description: RTCSessionDescription) -> Result<()> {
        Ok(self.inner.set_remote_description(description).await?)
    }
    pub async fn add_ice_candidate(&self, candidate: RTCIceCandidateInit) -> Result<()> {
        Ok(self.inner.add_ice_candidate(candidate).await?)
    }
    pub async fn create_data_channel(&self, label: &str, _: Option<()>) -> Result<Arc<Channel>> {
        let channel = self
            .callbacks
            .wrap(self.inner.create_data_channel(label, None).await?);
        channel.start_reader();
        Ok(channel)
    }
    pub async fn close(&self) -> Result<()> {
        self.callbacks.clear();
        Ok(self.inner.close().await?)
    }
}
impl Drop for Connection {
    fn drop(&mut self) {
        self.callbacks.clear();
        let inner = self.inner.clone();
        if let Ok(runtime) = tokio::runtime::Handle::try_current() {
            runtime.spawn(async move {
                let _ = inner.close().await;
            });
        }
    }
}

pub struct Channel {
    inner: Arc<dyn DataChannel>,
    message: Mutex<Option<Callback<RTCDataChannelMessage>>>,
    open: Mutex<Option<OpenCallback>>,
    opened: AtomicBool,
    stop: watch::Sender<bool>,
}
impl Channel {
    fn fire_open(&self) {
        if let Some(callback) = self.open.lock().unwrap().take() {
            tokio::spawn(callback());
        }
    }
    pub fn on_open(&self, callback: OpenCallback) {
        *self.open.lock().unwrap() = Some(callback);
        if self.opened.load(Ordering::Acquire) {
            self.fire_open();
        }
    }
    pub fn on_message(&self, callback: Callback<RTCDataChannelMessage>) {
        *self.message.lock().unwrap() = Some(callback);
    }
    pub async fn send(&self, payload: &Bytes) -> Result<()> {
        Ok(self.inner.send(BytesMut::from(payload.as_ref())).await?)
    }
    fn start_reader(self: &Arc<Self>) {
        let channel = self.clone();
        let mut stop = self.stop.subscribe();
        tokio::spawn(async move {
            loop {
                if *stop.borrow() {
                    break;
                }
                let event =
                    tokio::select! { _ = stop.changed() => break, e = channel.inner.poll() => e };
                match event {
                    Some(DataChannelEvent::OnOpen) => {
                        channel.opened.store(true, Ordering::Release);
                        channel.fire_open();
                    }
                    Some(DataChannelEvent::OnMessage(message)) => {
                        let future = channel
                            .message
                            .lock()
                            .unwrap()
                            .as_mut()
                            .map(|cb| cb(message));
                        if let Some(future) = future {
                            future.await;
                        }
                    }
                    Some(DataChannelEvent::OnClose) | None => break,
                    _ => {}
                }
            }
            channel.message.lock().unwrap().take();
            channel.open.lock().unwrap().take();
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::sync::oneshot;

    async fn full_description(pc: &Connection) -> RTCSessionDescription {
        tokio::time::timeout(std::time::Duration::from_secs(10), async {
            loop {
                let wait = pc.callbacks.gather_notify.notified();
                if pc.callbacks.gathered.load(Ordering::Acquire) {
                    break;
                }
                wait.await;
            }
            pc.inner.local_description().await.unwrap()
        })
        .await
        .expect("ICE gathering timed out")
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn current_dtls_transports_binary_messages_and_closes_readers() {
        tokio::time::timeout(std::time::Duration::from_secs(20), async {
            let caller = Connection::new(vec![]).await.unwrap();
            let answerer = Connection::new(vec![]).await.unwrap();
            answerer.on_data_channel(Box::new(|channel| {
                Box::pin(async move {
                    let echo = channel.clone();
                    channel.on_message(Box::new(move |message| {
                        let echo = echo.clone();
                        Box::pin(async move {
                            echo.send(&message.data.freeze()).await.unwrap();
                        })
                    }));
                })
            }));
            let channel = caller.create_data_channel("fedi3", None).await.unwrap();
            let (opened, ready) = oneshot::channel();
            channel.on_open(Box::new(move || {
                Box::pin(async move {
                    let _ = opened.send(());
                })
            }));
            let (received, data) = oneshot::channel();
            let mut received = Some(received);
            channel.on_message(Box::new(move |message| {
                let sender = received.take();
                Box::pin(async move {
                    if let Some(sender) = sender {
                        let _ = sender.send(message.data);
                    }
                })
            }));
            let offer = caller.create_offer(None).await.unwrap();
            caller.set_local_description(offer).await.unwrap();
            answerer
                .set_remote_description(full_description(&caller).await)
                .await
                .unwrap();
            let answer = answerer.create_answer(None).await.unwrap();
            answerer.set_local_description(answer).await.unwrap();
            caller
                .set_remote_description(full_description(&answerer).await)
                .await
                .unwrap();
            ready.await.unwrap();
            let payload = Bytes::from((0..32000).map(|i| (i % 251) as u8).collect::<Vec<_>>());
            channel.send(&payload).await.unwrap();
            assert_eq!(data.await.unwrap().as_ref(), payload.as_ref());
            let weak = Arc::downgrade(&channel);
            caller.close().await.unwrap();
            answerer.close().await.unwrap();
            drop(channel);
            for _ in 0..100 {
                if weak.upgrade().is_none() {
                    return;
                }
                tokio::time::sleep(std::time::Duration::from_millis(5)).await;
            }
            panic!("data channel reader kept its channel alive after shutdown");
        })
        .await
        .expect("DTLS/SCTP handshake or binary roundtrip timed out");
    }

    #[test]
    fn signaling_keeps_legacy_json_fields() {
        let legacy = serde_json::json!({"candidate":"candidate:1 1 udp 1 127.0.0.1 9999 typ host", "sdpMid":"0", "sdpMLineIndex":0, "usernameFragment":"old-peer"});
        let candidate: RTCIceCandidateInit = serde_json::from_value(legacy.clone()).unwrap();
        assert_eq!(serde_json::to_value(candidate).unwrap(), legacy);
        let legacy = serde_json::json!({"type":"offer", "sdp":"v=0\r\n"});
        let description: RTCSessionDescription = serde_json::from_value(legacy.clone()).unwrap();
        assert_eq!(serde_json::to_value(description).unwrap(), legacy);
    }
}

#[cfg(test)]
mod legacy_wire_compatibility {
    use super::*;
    use std::{
        io::{BufRead, BufReader, Write},
        process::{Child, ChildStdout, Command, Stdio},
    };
    use tokio::sync::oneshot;

    struct OldPeer(Child);
    impl Drop for OldPeer {
        fn drop(&mut self) {
            let _ = self.0.kill();
            let _ = self.0.wait();
        }
    }
    async fn read_description(
        mut output: BufReader<ChildStdout>,
    ) -> (RTCSessionDescription, BufReader<ChildStdout>) {
        tokio::task::spawn_blocking(move || {
            let mut line = String::new();
            output.read_line(&mut line).unwrap();
            (
                serde_json::from_str(&line).expect("old peer returned invalid SDP"),
                output,
            )
        })
        .await
        .unwrap()
    }
    async fn full_description(pc: &Connection) -> RTCSessionDescription {
        loop {
            let wait = pc.callbacks.gather_notify.notified();
            if pc.callbacks.gathered.load(Ordering::Acquire) {
                break;
            }
            wait.await;
        }
        pc.inner.local_description().await.unwrap()
    }
    fn observe(channel: &Channel, ready: oneshot::Sender<()>, received: oneshot::Sender<BytesMut>) {
        channel.on_open(Box::new(move || {
            Box::pin(async move {
                let _ = ready.send(());
            })
        }));
        let mut received = Some(received);
        channel.on_message(Box::new(move |message| {
            let sender = received.take();
            Box::pin(async move {
                if let Some(sender) = sender {
                    let _ = sender.send(message.data);
                }
            })
        }));
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    #[ignore = "requires the isolated old peer built by scripts/verify_legacy_webrtc.py"]
    async fn legacy_webrtc_012_interoperates_in_both_sdp_roles() {
        let executable =
            std::env::var("FEDI3_LEGACY_WEBRTC_PEER").expect("build the old test oracle first");
        for old_role in ["offer", "answer"] {
            tokio::time::timeout(std::time::Duration::from_secs(45), async {
                let mut old = OldPeer(
                    Command::new(&executable)
                        .arg(old_role)
                        .stdin(Stdio::piped())
                        .stdout(Stdio::piped())
                        .stderr(Stdio::inherit())
                        .spawn()
                        .unwrap(),
                );
                let mut input = old.0.stdin.take().unwrap();
                let output = BufReader::new(old.0.stdout.take().unwrap());
                let pc = Connection::new(vec![]).await.unwrap();
                let (ready_tx, ready_rx) = oneshot::channel();
                let (message_tx, message_rx) = oneshot::channel();
                let channel;
                if old_role == "offer" {
                    let (channel_tx, channel_rx) = oneshot::channel();
                    let mut hooks = Some((ready_tx, message_tx, channel_tx));
                    pc.on_data_channel(Box::new(move |channel| {
                        let hooks = hooks.take();
                        Box::pin(async move {
                            let (ready, message, sender) = hooks.unwrap();
                            observe(&channel, ready, message);
                            let _ = sender.send(channel);
                        })
                    }));
                    let (offer, _output) = read_description(output).await;
                    pc.set_remote_description(offer).await.unwrap();
                    pc.set_local_description(pc.create_answer(None).await.unwrap())
                        .await
                        .unwrap();
                    writeln!(
                        input,
                        "{}",
                        serde_json::to_string(&full_description(&pc).await).unwrap()
                    )
                    .unwrap();
                    input.flush().unwrap();
                    channel = channel_rx.await.unwrap();
                } else {
                    channel = pc.create_data_channel("fedi3", None).await.unwrap();
                    observe(&channel, ready_tx, message_tx);
                    pc.set_local_description(pc.create_offer(None).await.unwrap())
                        .await
                        .unwrap();
                    writeln!(
                        input,
                        "{}",
                        serde_json::to_string(&full_description(&pc).await).unwrap()
                    )
                    .unwrap();
                    input.flush().unwrap();
                    let (answer, _output) = read_description(output).await;
                    pc.set_remote_description(answer).await.unwrap();
                }
                ready_rx.await.unwrap();
                // The unchanged v1 framing from webrtc_p2p: version, id, total, offset, payload.
                let body = b"{\"kind\":\"legacy-message\",\"text\":\"profilo esistente\"}";
                let mut frame = vec![1];
                frame.extend_from_slice(&[7; 16]);
                frame.extend_from_slice(&(body.len() as u32).to_le_bytes());
                frame.extend_from_slice(&0u32.to_le_bytes());
                frame.extend_from_slice(body);
                channel.send(&Bytes::copy_from_slice(&frame)).await.unwrap();
                assert_eq!(
                    message_rx.await.unwrap().as_ref(),
                    frame,
                    "old {old_role} changed binary frames"
                );
                pc.close().await.unwrap();
                writeln!(input, "done").unwrap();
                input.flush().unwrap();
                drop(old);
            })
            .await
            .unwrap_or_else(|_| panic!("WebRTC 0.12 interoperability timed out (old {old_role})"));
        }
    }
}
