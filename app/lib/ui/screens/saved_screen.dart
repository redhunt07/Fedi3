/*
 * SPDX-FileCopyrightText: 2026 RedHunt07 - FEDI3 Project
 * SPDX-License-Identifier: AGPL-3.0-only
 */

import 'package:material_ui/material_ui.dart';

import '../../l10n/l10n_ext.dart';
import '../../model/note_models.dart';
import '../../state/app_state.dart';
import '../../state/saved_notes_store.dart';
import '../theme/ui_tokens.dart';
import '../widgets/fedi3_ui.dart';
import '../widgets/note_card.dart';

class SavedScreen extends StatefulWidget {
  const SavedScreen({super.key, required this.appState});

  final AppState appState;

  @override
  State<SavedScreen> createState() => _SavedScreenState();
}

class _SavedScreenState extends State<SavedScreen> {
  List<Map<String, dynamic>> _entries = const [];
  bool _loading = true;

  @override
  void initState() {
    super.initState();
    _load();
  }

  Future<void> _load() async {
    if (mounted) setState(() => _loading = true);
    final entries = await SavedNotesStore.readAll();
    if (!mounted) return;
    setState(() {
      _entries = entries;
      _loading = false;
    });
  }

  Future<void> _remove(String id) async {
    await SavedNotesStore.remove(id);
    await _load();
  }

  @override
  Widget build(BuildContext context) {
    return Scaffold(
      appBar: AppBar(
        title: Text(context.l10n.savedTitle),
        actions: [
          IconButton(
            tooltip: context.l10n.savedRefresh,
            onPressed: _loading ? null : _load,
            icon: const Icon(Icons.refresh_rounded),
          ),
        ],
      ),
      body: _loading
          ? const Center(child: CircularProgressIndicator())
          : _entries.isEmpty
              ? EmptyState(
                  icon: Icons.bookmark_border_rounded,
                  title: context.l10n.savedEmptyTitle,
                  message: context.l10n.savedEmptyHint,
                )
              : RefreshIndicator(
                  onRefresh: _load,
                  child: ListView.separated(
                    padding: const EdgeInsets.all(16),
                    itemCount: _entries.length,
                    separatorBuilder: (_, __) => const SizedBox(height: 12),
                    itemBuilder: (context, index) {
                      final entry = _entries[index];
                      final activity =
                          (entry['activity'] as Map).cast<String, dynamic>();
                      final item = TimelineItem.tryFromActivity(activity);
                      if (item == null) return const SizedBox.shrink();
                      final id = entry['id']?.toString() ?? item.note.id;
                      return Dismissible(
                        key: ValueKey('saved-$id'),
                        direction: DismissDirection.endToStart,
                        background: Container(
                          alignment: Alignment.centerRight,
                          padding: const EdgeInsets.only(right: 20),
                          decoration: BoxDecoration(
                            color: Theme.of(context).colorScheme.errorContainer,
                            borderRadius:
                                BorderRadius.circular(UiTokens.radiusCard),
                          ),
                          child: Icon(Icons.delete_outline_rounded,
                              color: Theme.of(context)
                                  .colorScheme
                                  .onErrorContainer),
                        ),
                        onDismissed: (_) => _remove(id),
                        child: NoteCard(
                          appState: widget.appState,
                          item: item,
                          rawActivity: activity,
                        ),
                      );
                    },
                  ),
                ),
    );
  }
}
