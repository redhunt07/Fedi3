$ErrorActionPreference = 'Stop'
$CoreUrl = 'http://127.0.0.1:8788'
$ConfigPath = "$env:APPDATA\Fedi3\config.json"
if (-not (Test-Path $ConfigPath)) {
    Write-Error "config.json non trovato in $ConfigPath"
    exit 1
}
$Config = Get-Content $ConfigPath | ConvertFrom-Json
$Token = $Config.internalToken
$RelayBase = $Config.publicBaseUrl
if ([string]::IsNullOrWhiteSpace($RelayBase)) {
    $RelayWs = $Config.relayWs
    if ($RelayWs -match '^wss://') { $RelayBase = $RelayWs -replace '^wss://', 'https://' }
    elseif ($RelayWs -match '^ws://') { $RelayBase = $RelayWs -replace '^ws://', 'http://' }
}
if (-not [string]::IsNullOrWhiteSpace($RelayBase)) {
    $RelayBase = $RelayBase.TrimEnd('/')
}
if ([string]::IsNullOrWhiteSpace($Token)) {
    Write-Error "internalToken non impostato (controlla le impostazioni dell'app)"
    exit 1
}

Write-Host "=== TURN log ===" -ForegroundColor Cyan
docker compose logs --tail 50 turn

Write-Host "`n=== Relay log (WebRTC / relay_fallback) ===" -ForegroundColor Cyan
docker compose logs --tail 100 relay | Select-String -Pattern 'webrtc|relay_fallback|tunnel'

Write-Host "`n=== Test POST /_fedi3/webrtc/send ===" -ForegroundColor Cyan
$offer = @{
    session_id = "test-$([DateTimeOffset]::Now.ToUnixTimeSeconds())"
    to_peer_id = "diagnose-peer"
    kind = "offer"
    payload = @{ sdp = "v=0 o=- 0 0 IN IP4 127.0.0.1 s=Fedi3 t=0 0 m=application 9 DTLS/SCTP 5000" }
} | ConvertTo-Json

if (-not [string]::IsNullOrWhiteSpace($RelayBase)) {
  try {
      Invoke-RestMethod -Uri "$RelayBase/_fedi3/webrtc/send" `
        -Method Post `
        -Headers @{ 'Content-Type' = 'application/json' } `
        -Body $offer | ConvertTo-Json
  } catch {
      Write-Host "Errore durante POST /_fedi3/webrtc/send (relay):"
      $resp = $_.Exception.Response
      if ($resp -ne $null) {
          $bodyStream = $resp.GetResponseStream()
          if ($bodyStream -ne $null) {
              $reader = New-Object System.IO.StreamReader($bodyStream)
              Write-Host $reader.ReadToEnd()
              $reader.Close()
              $bodyStream.Close()
          } else {
              Write-Host "Nessun corpo nella risposta"
          }
      } else {
          Write-Host $_.Exception.Message
      }
  }
} else {
  Write-Host "RelayBase non configurato in config.json (publicBaseUrl/relayWs)"
}

Write-Host "`n=== Relay UPnP telemetry ===" -ForegroundColor Cyan
try {
    if (-not [string]::IsNullOrWhiteSpace($RelayBase)) {
        $relays = Invoke-RestMethod -Uri "$RelayBase/_fedi3/relay/relays" -Method Get
        if ($relays.relays) {
            $relays.relays |
                Select-Object @{ Name = 'relay'; Expression = { $_.relay_url } },
                              @{ Name = 'upnpStart'; Expression = { $_.telemetry.p2p_upnp_port_start } },
                              @{ Name = 'upnpEnd'; Expression = { $_.telemetry.p2p_upnp_port_end } } |
                Format-Table -AutoSize
        } else {
            Write-Host "Nessun relay disponibile in /_fedi3/relay/relays"
        }

        Write-Host "`n=== Relay stats (UPnP range) ===" -ForegroundColor Cyan
        $stats = Invoke-RestMethod -Uri "$RelayBase/_fedi3/relay/stats" -Method Get
        $stats | Select-Object relay_url, p2p_upnp_port_start, p2p_upnp_port_end | Format-List
    } else {
        Write-Host "RelayBase non configurato in config.json (publicBaseUrl/relayWs)"
    }
} catch {
    Write-Host "Errore durante GET /_fedi3/relay/relays o /_fedi3/relay/stats:"
    Write-Host $_.Exception.Message
}
