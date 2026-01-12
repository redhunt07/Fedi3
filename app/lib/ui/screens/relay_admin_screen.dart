/*
 * SPDX-FileCopyrightText: 2026 RedHunt07 - FEDI3 Project
 * SPDX-License-Identifier: AGPL-3.0-only
 */

import 'dart:convert';
import 'dart:io';
import 'dart:math';

import 'package:flutter/material.dart';
import 'package:path_provider/path_provider.dart';

import '../../l10n/l10n_ext.dart';
import '../../services/relay_admin_api.dart';
import '../../state/app_state.dart';
import '../widgets/network_error_card.dart';

class RelayAdminScreen extends StatefulWidget {
  const RelayAdminScreen({super.key, required this.appState});

  final AppState appState;

  @override
  State<RelayAdminScreen> createState() => _RelayAdminScreenState();
}

class _RelayAdminScreenState extends State<RelayAdminScreen> with SingleTickerProviderStateMixin {
  late final TabController _tabs = TabController(length: 3, vsync: this);
  bool _loading = false;
  String? _error;
  List<Map<String, dynamic>> _users = const [];
  List<Map<String, dynamic>> _audit = const [];
  String _userQuery = '';
  int _usersOffset = 0;
  int _auditOffset = 0;
  String _auditQuery = '';
  bool _auditOnlyFailed = false;
  bool _auditReverse = false;
  final TextEditingController _adminToken = TextEditingController();
  final TextEditingController _newUser = TextEditingController();
  final TextEditingController _newToken = TextEditingController();

  RelayAdminApi? _api;

  @override
  void initState() {
    super.initState();
    _adminToken.text = widget.appState.prefs.relayAdminToken;
    _reloadApi();
    _refreshUsers(reset: true);
    _refreshAudit(reset: true);
  }

  @override
  void dispose() {
    _tabs.dispose();
    _adminToken.dispose();
    _newUser.dispose();
    _newToken.dispose();
    super.dispose();
  }

  void _reloadApi() {
    final token = _adminToken.text.trim();
    if (token.isEmpty) {
      _api = null;
      return;
    }
    _api = RelayAdminApi(
      relayWs: widget.appState.config!.relayWs,
      adminToken: token,
    );
  }

  Future<void> _saveToken() async {
    await widget.appState.savePrefs(
      widget.appState.prefs.copyWith(relayAdminToken: _adminToken.text.trim()),
    );
    _reloadApi();
    if (mounted) setState(() {});
  }

  Future<void> _refreshUsers({required bool reset}) async {
    final api = _api;
    if (api == null) return;
    setState(() {
      _loading = true;
      _error = null;
      if (reset) {
        _usersOffset = 0;
      }
    });
    try {
      final items = await api.listUsers(limit: 200, offset: _usersOffset);
      final filtered = _userQuery.trim().isEmpty
          ? items
          : items.where((u) => u['username']?.toString().contains(_userQuery.trim()) ?? false).toList();
      if (!mounted) return;
      setState(() {
        _users = reset ? filtered : [..._users, ...filtered];
        if (items.isNotEmpty) {
          _usersOffset += items.length;
        }
      });
    } catch (e) {
      if (!mounted) return;
      setState(() => _error = e.toString());
    } finally {
      if (mounted) setState(() => _loading = false);
    }
  }

  Future<void> _refreshAudit({required bool reset}) async {
    final api = _api;
    if (api == null) return;
    setState(() {
      _loading = true;
      _error = null;
      if (reset) {
        _auditOffset = 0;
      }
    });
    try {
      final items = await api.listAudit(limit: 200, offset: _auditOffset);
      if (!mounted) return;
      setState(() {
        _audit = reset ? items : [..._audit, ...items];
        if (items.isNotEmpty) {
          _auditOffset += items.length;
        }
      });
    } catch (e) {
      if (!mounted) return;
      setState(() => _error = e.toString());
    } finally {
      if (mounted) setState(() => _loading = false);
    }
  }

  Future<void> _exportAudit() async {
    if (_audit.isEmpty) return;
    final dir = await getTemporaryDirectory();
    final ts = DateTime.now().toUtc().toIso8601String().replaceAll(':', '-');
    final file = File('${dir.path}${Platform.pathSeparator}fedi3-relay-audit-$ts.json');
    await file.writeAsString(jsonEncode(_audit));
    if (!mounted) return;
    ScaffoldMessenger.of(context).showSnackBar(
      SnackBar(content: Text(context.l10n.relayAdminAuditExported(file.path))),
    );
  }

  Future<void> _toggleDisable(Map<String, dynamic> user, bool disabled) async {
    final api = _api;
    if (api == null) return;
    final username = user['username']?.toString() ?? '';
    if (username.isEmpty) return;
    setState(() {
      _loading = true;
      _error = null;
    });
    try {
      if (disabled) {
        await api.disableUser(username);
      } else {
        await api.enableUser(username);
      }
      await _refreshUsers(reset: true);
    } catch (e) {
      if (!mounted) return;
      setState(() => _error = e.toString());
    } finally {
      if (mounted) setState(() => _loading = false);
    }
  }

  Future<void> _rotateToken(Map<String, dynamic> user) async {
    final api = _api;
    if (api == null) return;
    final username = user['username']?.toString() ?? '';
    if (username.isEmpty) return;
    setState(() {
      _loading = true;
      _error = null;
    });
    try {
      final token = await api.rotateToken(username);
      if (!mounted) return;
      await showDialog<void>(
        context: context,
        builder: (context) => AlertDialog(
          title: Text(context.l10n.relayAdminRotate),
          content: SelectableText(token),
          actions: [
            TextButton(onPressed: () => Navigator.pop(context), child: Text(context.l10n.ok)),
          ],
        ),
      );
    } catch (e) {
      if (!mounted) return;
      setState(() => _error = e.toString());
    } finally {
      if (mounted) setState(() => _loading = false);
    }
  }

  Future<void> _deleteUser(Map<String, dynamic> user) async {
    final api = _api;
    if (api == null) return;
    final username = user['username']?.toString() ?? '';
    if (username.isEmpty) return;
    final confirmed = await showDialog<bool>(
      context: context,
      builder: (context) => AlertDialog(
        title: Text(context.l10n.relayAdminDelete),
        content: Text(context.l10n.relayAdminDeleteConfirm(username)),
        actions: [
          TextButton(onPressed: () => Navigator.pop(context, false), child: Text(context.l10n.cancel)),
          FilledButton(onPressed: () => Navigator.pop(context, true), child: Text(context.l10n.relayAdminDelete)),
        ],
      ),
    );
    if (confirmed != true) return;
    setState(() {
      _loading = true;
      _error = null;
    });
    try {
      await api.deleteUser(username);
      await _refreshUsers(reset: true);
    } catch (e) {
      if (!mounted) return;
      setState(() => _error = e.toString());
    } finally {
      if (mounted) setState(() => _loading = false);
    }
  }

  Future<void> _registerUser() async {
    final api = _api;
    if (api == null) return;
    final username = _newUser.text.trim();
    final token = _newToken.text.trim();
    if (username.isEmpty || token.isEmpty) return;
    setState(() {
      _loading = true;
      _error = null;
    });
    try {
      await api.registerUser(username, token);
      _newUser.clear();
      _newToken.clear();
      await _refreshUsers(reset: true);
    } catch (e) {
      if (!mounted) return;
      setState(() => _error = e.toString());
    } finally {
      if (mounted) setState(() => _loading = false);
    }
  }

  void _generateToken() {
    final rand = Random.secure();
    final bytes = List<int>.generate(24, (_) => rand.nextInt(256));
    _newToken.text = bytes.map((b) => b.toRadixString(16).padLeft(2, '0')).join();
    setState(() {});
  }

  @override
  Widget build(BuildContext context) {
    final relayWs = widget.appState.config?.relayWs ?? '';
    final hasToken = _adminToken.text.trim().isNotEmpty;
    return Scaffold(
      appBar: AppBar(
        title: Text(context.l10n.relayAdminTitle),
        bottom: TabBar(
          controller: _tabs,
          tabs: [
            Tab(text: context.l10n.relayAdminUsers),
            Tab(text: context.l10n.relayAdminAudit),
            Tab(text: context.l10n.relayAdminRegister),
          ],
        ),
        actions: [
          IconButton(
            tooltip: context.l10n.telemetryRefresh,
            onPressed: _loading
                ? null
                : () async {
                    if (_tabs.index == 0) await _refreshUsers(reset: true);
                    if (_tabs.index == 1) await _refreshAudit(reset: true);
                  },
            icon: const Icon(Icons.refresh),
          ),
          if (_tabs.index == 1)
            IconButton(
              tooltip: context.l10n.relayAdminAuditExport,
              onPressed: _loading ? null : _exportAudit,
              icon: const Icon(Icons.upload_file_outlined),
            ),
        ],
      ),
      body: Column(
        children: [
          Padding(
            padding: const EdgeInsets.fromLTRB(16, 12, 16, 8),
            child: Column(
              crossAxisAlignment: CrossAxisAlignment.start,
              children: [
                Text(context.l10n.relayAdminRelayWsLabel),
                const SizedBox(height: 4),
                Text(relayWs, style: TextStyle(color: Theme.of(context).colorScheme.onSurface.withAlpha(150))),
                const SizedBox(height: 12),
                Text(context.l10n.relayAdminTokenLabel),
                const SizedBox(height: 6),
                Row(
                  children: [
                    Expanded(
                      child: TextField(
                        controller: _adminToken,
                        obscureText: true,
                        enableSuggestions: false,
                        autocorrect: false,
                        decoration: InputDecoration(
                          hintText: context.l10n.relayAdminTokenHint,
                          border: const OutlineInputBorder(),
                        ),
                        onChanged: (_) => setState(() {}),
                      ),
                    ),
                    const SizedBox(width: 8),
                    FilledButton(
                      onPressed: _saveToken,
                      child: Text(context.l10n.save),
                    ),
                  ],
                ),
                if (!hasToken)
                  Padding(
                    padding: const EdgeInsets.only(top: 8),
                    child: Text(
                      context.l10n.relayAdminTokenMissing,
                      style: TextStyle(color: Theme.of(context).colorScheme.error),
                    ),
                  ),
                if (_error != null)
                  Padding(
                    padding: const EdgeInsets.only(top: 8),
                    child: NetworkErrorCard(
                      message: _error,
                      compact: true,
                      onRetry: () async {
                        if (_tabs.index == 0) await _refreshUsers(reset: true);
                        if (_tabs.index == 1) await _refreshAudit(reset: true);
                      },
                    ),
                  ),
              ],
            ),
          ),
          if (hasToken) _buildStats(context),
          Expanded(
            child: TabBarView(
              controller: _tabs,
              children: [
                _buildUsersTab(context, hasToken),
                _buildAuditTab(context, hasToken),
                _buildRegisterTab(context, hasToken),
              ],
            ),
          ),
        ],
      ),
    );
  }

  Widget _buildUsersTab(BuildContext context, bool hasToken) {
    if (!hasToken) {
      return Center(child: Text(context.l10n.relayAdminTokenMissing));
    }
    if (_loading && _users.isEmpty) {
      return const Center(child: CircularProgressIndicator());
    }
    return Column(
      children: [
        Padding(
          padding: const EdgeInsets.fromLTRB(16, 12, 16, 0),
          child: TextField(
            decoration: InputDecoration(
              prefixIcon: const Icon(Icons.search),
              hintText: context.l10n.relayAdminUserSearchHint,
            ),
            onChanged: (v) {
              _userQuery = v;
              _refreshUsers(reset: true);
            },
          ),
        ),
        Expanded(
          child: ListView.separated(
            padding: const EdgeInsets.all(16),
            itemCount: _users.length + 1,
            separatorBuilder: (_, __) => const SizedBox(height: 8),
            itemBuilder: (context, index) {
              if (index >= _users.length) {
                return Center(
                  child: OutlinedButton(
                    onPressed: _loading ? null : () => _refreshUsers(reset: false),
                    child: Text(context.l10n.listLoadMore),
                  ),
                );
              }
              final user = _users[index];
              final username = user['username']?.toString() ?? '';
              final disabled = user['disabled'] == true;
              return Card(
                child: ListTile(
                  title: Text(username),
                  subtitle: Text(disabled ? context.l10n.relayAdminUserDisabled : context.l10n.relayAdminUserEnabled),
                  trailing: Wrap(
                    spacing: 6,
                    children: [
                      IconButton(
                        tooltip: context.l10n.relayAdminRotate,
                        onPressed: _loading ? null : () => _rotateToken(user),
                        icon: const Icon(Icons.vpn_key_outlined),
                      ),
                      IconButton(
                        tooltip: disabled ? context.l10n.relayAdminEnable : context.l10n.relayAdminDisable,
                        onPressed: _loading ? null : () => _toggleDisable(user, !disabled),
                        icon: Icon(disabled ? Icons.play_arrow : Icons.pause),
                      ),
                      IconButton(
                        tooltip: context.l10n.relayAdminDelete,
                        onPressed: _loading ? null : () => _deleteUser(user),
                        icon: const Icon(Icons.delete_outline),
                      ),
                    ],
                  ),
                  onTap: _loading
                      ? null
                      : () async {
                          final api = _api;
                          if (api == null) return;
                          final detail = await api.getUser(username);
                          if (!context.mounted) return;
                          await showDialog<void>(
                            context: context,
                            builder: (_) => AlertDialog(
                              title: Text(username),
                              content: Text(detail.toString()),
                              actions: [
                                TextButton(onPressed: () => Navigator.pop(context), child: Text(context.l10n.ok)),
                              ],
                            ),
                          );
                        },
                ),
              );
            },
          ),
        ),
      ],
    );
  }

  Widget _buildStats(BuildContext context) {
    final totalUsers = _users.length;
    final disabled = _users.where((u) => u['disabled'] == true).length;
    final lastAuditMs = _audit.isNotEmpty ? _audit.first['created_at_ms']?.toString() : null;
    return Padding(
      padding: const EdgeInsets.fromLTRB(16, 4, 16, 4),
      child: Wrap(
        spacing: 8,
        runSpacing: 8,
        children: [
          _chip(context.l10n.relayAdminUsersCount(totalUsers)),
          _chip(context.l10n.relayAdminUsersDisabledCount(disabled)),
          if (lastAuditMs != null) _chip('${context.l10n.relayAdminAuditLast}: $lastAuditMs'),
        ],
      ),
    );
  }

  Widget _buildAuditTab(BuildContext context, bool hasToken) {
    if (!hasToken) {
      return Center(child: Text(context.l10n.relayAdminTokenMissing));
    }
    if (_loading && _audit.isEmpty) {
      return const Center(child: CircularProgressIndicator());
    }
    var filtered = _audit;
    final q = _auditQuery.trim().toLowerCase();
    if (q.isNotEmpty) {
      filtered = filtered.where((ev) {
        final hay = [
          ev['action'],
          ev['username'],
          ev['actor'],
          ev['ip'],
          ev['detail'],
        ].whereType<Object>().map((v) => v.toString().toLowerCase());
        return hay.any((v) => v.contains(q));
      }).toList();
    }
    if (_auditOnlyFailed) {
      filtered = filtered.where((ev) => ev['ok'] != true).toList();
    }
    if (_auditReverse) {
      filtered = filtered.reversed.toList();
    }
    return Column(
      children: [
        Padding(
          padding: const EdgeInsets.fromLTRB(16, 12, 16, 0),
          child: Row(
            children: [
              Expanded(
                child: TextField(
                  decoration: InputDecoration(
                    prefixIcon: const Icon(Icons.search),
                    hintText: context.l10n.relayAdminAuditSearchHint,
                  ),
                  onChanged: (v) => setState(() => _auditQuery = v),
                ),
              ),
              const SizedBox(width: 8),
              IconButton(
                tooltip: context.l10n.relayAdminAuditFailedOnly,
                onPressed: () => setState(() => _auditOnlyFailed = !_auditOnlyFailed),
                icon: Icon(_auditOnlyFailed ? Icons.filter_alt : Icons.filter_alt_outlined),
              ),
              IconButton(
                tooltip: context.l10n.relayAdminAuditReverse,
                onPressed: () => setState(() => _auditReverse = !_auditReverse),
                icon: Icon(_auditReverse ? Icons.swap_vert : Icons.swap_vert_outlined),
              ),
            ],
          ),
        ),
        Expanded(
          child: ListView.separated(
            padding: const EdgeInsets.all(16),
            itemCount: filtered.length + 1,
            separatorBuilder: (_, __) => const SizedBox(height: 8),
            itemBuilder: (context, index) {
              if (index >= filtered.length) {
                return Center(
                  child: OutlinedButton(
                    onPressed: _loading ? null : () => _refreshAudit(reset: false),
                    child: Text(context.l10n.listLoadMore),
                  ),
                );
              }
              final ev = filtered[index];
              final ok = ev['ok'] == true;
              return Card(
                child: ListTile(
                  title: Text('${ev['action']} · ${ok ? context.l10n.ok : context.l10n.relayAdminAuditFailed}'),
                  subtitle: Text(ev.toString()),
                ),
              );
            },
          ),
        ),
      ],
    );
  }

  Widget _chip(String label) {
    return Chip(label: Text(label));
  }

  Widget _buildRegisterTab(BuildContext context, bool hasToken) {
    if (!hasToken) {
      return Center(child: Text(context.l10n.relayAdminTokenMissing));
    }
    return ListView(
      padding: const EdgeInsets.all(16),
      children: [
        TextField(
          controller: _newUser,
          decoration: InputDecoration(
            labelText: context.l10n.relayAdminUsername,
            hintText: context.l10n.relayAdminRegisterHint,
          ),
        ),
        const SizedBox(height: 12),
        TextField(
          controller: _newToken,
          decoration: InputDecoration(
            labelText: context.l10n.relayAdminTokenLabel,
          ),
        ),
        const SizedBox(height: 8),
        Row(
          children: [
            OutlinedButton(
              onPressed: _generateToken,
              child: Text(context.l10n.relayAdminGenerateToken),
            ),
            const Spacer(),
            FilledButton(
              onPressed: _loading ? null : _registerUser,
              child: Text(context.l10n.relayAdminRegister),
            ),
          ],
        ),
      ],
    );
  }
}
