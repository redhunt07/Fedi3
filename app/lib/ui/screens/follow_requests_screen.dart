/*
 * SPDX-FileCopyrightText: 2026 RedHunt07 - FEDI3 Project
 * SPDX-License-Identifier: AGPL-3.0-only
 */

import 'package:material_ui/material_ui.dart';

import '../../core/core_api.dart';
import '../../l10n/gen/app_localizations.dart';
import '../../l10n/l10n_ext.dart';
import '../../model/follow_request.dart';
import '../../state/app_state.dart';
import '../widgets/network_error_card.dart';

/// H-44: approve or reject pending follow requests for a locked account.
class FollowRequestsScreen extends StatefulWidget {
  const FollowRequestsScreen({super.key, required this.appState});

  final AppState appState;

  @override
  State<FollowRequestsScreen> createState() => _FollowRequestsScreenState();
}

class _FollowRequestsScreenState extends State<FollowRequestsScreen> {
  bool _loading = false;
  String? _error;
  final _items = <FollowRequest>[];
  final _busy = <String>{};

  @override
  void initState() {
    super.initState();
    _load();
  }

  Future<void> _load() async {
    if (!widget.appState.isRunning) {
      setState(() {
        _loading = false;
        _error = null;
        _items.clear();
      });
      return;
    }
    final cfg = widget.appState.config;
    if (cfg == null) return;
    setState(() {
      _loading = true;
      _error = null;
    });
    try {
      final items = await CoreApi(config: cfg).fetchFollowRequests();
      if (!mounted) return;
      setState(() {
        _items
          ..clear()
          ..addAll(items);
        _loading = false;
      });
    } on CoreApiException catch (e) {
      if (!mounted) return;
      setState(() {
        _loading = false;
        _error = e.message;
      });
    } catch (e) {
      if (!mounted) return;
      setState(() {
        _loading = false;
        _error = '$e';
      });
    }
  }

  Future<void> _decide(FollowRequest request, {required bool approve}) async {
    final cfg = widget.appState.config;
    if (cfg == null) return;
    setState(() => _busy.add(request.actor));
    final messenger = ScaffoldMessenger.of(context);
    final l10n = context.l10n;
    try {
      final api = CoreApi(config: cfg);
      if (approve) {
        await api.approveFollowRequest(request.actor);
      } else {
        await api.rejectFollowRequest(request.actor);
      }
      if (!mounted) return;
      setState(() => _items.removeWhere((r) => r.actor == request.actor));
      messenger.showSnackBar(
        SnackBar(
          content: Text(
            approve
                ? l10n.followRequestsApproved(request.handle)
                : l10n.followRequestsRejected(request.handle),
          ),
        ),
      );
    } on CoreApiException catch (e) {
      if (!mounted) return;
      messenger.showSnackBar(SnackBar(content: Text(e.message)));
    } finally {
      if (mounted) {
        setState(() => _busy.remove(request.actor));
      }
    }
  }

  @override
  Widget build(BuildContext context) {
    final l10n = context.l10n;
    return Scaffold(
      appBar: AppBar(
        title: Text(l10n.followRequestsTitle),
        actions: [
          IconButton(
            tooltip: l10n.settingsFollowRequestsHint,
            onPressed: widget.appState.isRunning ? _load : null,
            icon: const Icon(Icons.refresh),
          ),
        ],
      ),
      body: _buildBody(l10n),
    );
  }

  Widget _buildBody(AppLocalizations l10n) {
    if (!widget.appState.isRunning) {
      return Center(
        child: Padding(
          padding: const EdgeInsets.all(24),
          child: Text(
            l10n.followRequestsCoreStopped,
            textAlign: TextAlign.center,
          ),
        ),
      );
    }
    if (_loading) {
      return const Center(child: CircularProgressIndicator());
    }
    if (_error != null) {
      return SingleChildScrollView(
        padding: const EdgeInsets.all(16),
        child: NetworkErrorCard(
          message: l10n.followRequestsLoadFailed(_error!),
          onRetry: _load,
        ),
      );
    }
    if (_items.isEmpty) {
      return Center(
        child: Padding(
          padding: const EdgeInsets.all(24),
          child: Text(l10n.followRequestsEmpty, textAlign: TextAlign.center),
        ),
      );
    }
    return RefreshIndicator(
      onRefresh: _load,
      child: ListView.separated(
        padding: const EdgeInsets.all(16),
        itemCount: _items.length,
        separatorBuilder: (_, __) => const SizedBox(height: 8),
        itemBuilder: (context, index) {
          final request = _items[index];
          final busy = _busy.contains(request.actor);
          return Card(
            child: ListTile(
              title: Text(request.handle),
              subtitle: Text(
                request.createdAt == null
                    ? request.actor
                    : '${request.actor}\n${request.createdAt}',
                maxLines: 2,
                overflow: TextOverflow.ellipsis,
              ),
              isThreeLine: request.createdAt != null,
              trailing: Wrap(
                spacing: 4,
                children: [
                  IconButton(
                    tooltip: l10n.followRequestsApprove,
                    onPressed:
                        busy ? null : () => _decide(request, approve: true),
                    icon: const Icon(Icons.check_circle_outline),
                  ),
                  IconButton(
                    tooltip: l10n.followRequestsReject,
                    onPressed:
                        busy ? null : () => _decide(request, approve: false),
                    icon: const Icon(Icons.cancel_outlined),
                  ),
                ],
              ),
            ),
          );
        },
      ),
    );
  }
}
