/// A transport peer is a device; the actor URL identifies the account.
List<Map<String, dynamic>> groupPeersByAccount(List<dynamic> peers) {
  final groups = <String, Map<String, dynamic>>{};
  for (final raw in peers.whereType<Map>()) {
    final peer = Map<String, dynamic>.from(raw);
    final actor =
        (peer['actor_url']?.toString() ?? '').replaceAll(RegExp(r'/+$'), '');
    final id = peer['peer_id']?.toString() ?? '';
    final key = actor.isEmpty ? 'peer:$id' : actor;
    final group = groups.putIfAbsent(
        key,
        () => {
              'username': peer['username']?.toString() ?? '',
              'actor_url': actor,
              'online': false,
              'devices': <Map<String, dynamic>>[]
            });
    final devices = group['devices'] as List<Map<String, dynamic>>;
    if (!devices.any((p) => p['peer_id'] == id)) devices.add(peer);
    group['online'] = group['online'] == true || peer['online'] == true;
  }
  return groups.values.toList();
}
