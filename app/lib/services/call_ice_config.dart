const defaultCallIceUrls = [
  'stun:stun.cloudflare.com:3478',
  'stun:stun.l.google.com:19302',
];

/// Keep one URL per ICE server. The Linux native WebRTC bridge retains only
/// the final URL when a server's `urls` value is a list.
List<Map<String, dynamic>> buildCallIceServers(
  List<String> urls, {
  String? username,
  String? credential,
}) {
  final configured =
      urls.map((url) => url.trim()).where((url) => url.isNotEmpty).toSet();
  final effective = configured.isEmpty ? defaultCallIceUrls : configured;
  return effective.map((url) {
    final turn = url.toLowerCase().startsWith('turn:') ||
        url.toLowerCase().startsWith('turns:');
    return <String, dynamic>{
      'urls': url,
      if (turn && username != null && username.isNotEmpty) 'username': username,
      if (turn && credential != null && credential.isNotEmpty)
        'credential': credential,
    };
  }).toList(growable: false);
}
