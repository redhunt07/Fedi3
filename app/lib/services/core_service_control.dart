/*
 * SPDX-FileCopyrightText: 2026 RedHunt07 - FEDI3 Project
 * SPDX-License-Identifier: AGPL-3.0-only
 */

import 'dart:io';

class CoreServiceRestartResult {
  const CoreServiceRestartResult({
    required this.ok,
    required this.message,
    this.manualCommand = '',
  });

  final bool ok;
  final String message;
  final String manualCommand;
}

class CoreServiceControl {
  CoreServiceControl._();

  static const String _winTaskName = 'Fedi3 Core';
  static const String _linuxServiceName = 'fedi3-core.service';

  static Future<CoreServiceRestartResult> restartBackgroundService() async {
    if (Platform.isWindows) {
      return _restartWindowsTask();
    }
    if (Platform.isLinux) {
      return _restartLinuxService();
    }
    return const CoreServiceRestartResult(
      ok: false,
      message: 'Platform not supported for background service restart.',
    );
  }

  static Future<CoreServiceRestartResult> _restartWindowsTask() async {
    const manual = 'schtasks /Run /TN "Fedi3 Core"';
    try {
      await Process.run('schtasks', ['/End', '/TN', _winTaskName]);
    } catch (_) {
      // Best-effort: task may not be running.
    }
    try {
      final run = await Process.run('schtasks', ['/Run', '/TN', _winTaskName]);
      if (run.exitCode == 0) {
        return const CoreServiceRestartResult(
          ok: true,
          message: 'Background core task restart requested.',
          manualCommand: manual,
        );
      }
      final stderr = (run.stderr ?? '').toString().trim();
      return CoreServiceRestartResult(
        ok: false,
        message: stderr.isNotEmpty
            ? stderr
            : 'Failed to restart Scheduled Task "$_winTaskName".',
        manualCommand: manual,
      );
    } catch (e) {
      return CoreServiceRestartResult(
        ok: false,
        message: 'Failed to execute schtasks: $e',
        manualCommand: manual,
      );
    }
  }

  static Future<CoreServiceRestartResult> _restartLinuxService() async {
    const manual = 'systemctl --user restart fedi3-core.service';
    try {
      final run = await Process.run('systemctl', [
        '--user',
        'restart',
        _linuxServiceName,
      ]);
      if (run.exitCode == 0) {
        return const CoreServiceRestartResult(
          ok: true,
          message: 'Background core service restarted.',
          manualCommand: manual,
        );
      }
      final stderr = (run.stderr ?? '').toString().trim();
      return CoreServiceRestartResult(
        ok: false,
        message: stderr.isNotEmpty
            ? stderr
            : 'Failed to restart systemd user service "$_linuxServiceName".',
        manualCommand: manual,
      );
    } catch (e) {
      return CoreServiceRestartResult(
        ok: false,
        message: 'Failed to execute systemctl: $e',
        manualCommand: manual,
      );
    }
  }
}

