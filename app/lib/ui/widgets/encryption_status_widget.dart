/*
 * SPDX-FileCopyrightText: 2026 RedHunt07 - FEDI3 Project
 * SPDX-License-Identifier: AGPL-3.0-only
 */

import 'package:flutter/material.dart';
import 'package:fedi3/services/encryption_manager.dart';

/// Widget to display encryption status in the chat interface
class EncryptionStatusWidget extends StatefulWidget {
  const EncryptionStatusWidget({super.key});

  @override
  State<EncryptionStatusWidget> createState() => _EncryptionStatusWidgetState();
}

class _EncryptionStatusWidgetState extends State<EncryptionStatusWidget> {
  late EncryptionManager _encryptionManager;
  Map<String, dynamic> _status = {};
  bool _isLoading = true;

  @override
  void initState() {
    super.initState();
    _encryptionManager = EncryptionManager();
    _loadEncryptionStatus();
  }

  Future<void> _loadEncryptionStatus() async {
    setState(() => _isLoading = true);
    try {
      _status = await _encryptionManager.getEncryptionStatus();
    } catch (e) {
      _status = {
        'has_keys': false,
        'enabled': false,
        'public_key_available': false,
        'pq_available': false,
      };
    } finally {
      setState(() => _isLoading = false);
    }
  }

  @override
  Widget build(BuildContext context) {
    if (_isLoading) {
      return const Padding(
        padding: EdgeInsets.all(8.0),
        child: Row(
          children: [
            SizedBox(
              width: 16,
              height: 16,
              child: CircularProgressIndicator(strokeWidth: 2),
            ),
            SizedBox(width: 8),
            Text(
              'Caricamento stato crittografia...',
              style: TextStyle(fontSize: 12, color: Colors.grey),
            ),
          ],
        ),
      );
    }

    final hasKeys = _status['has_keys'] ?? false;
    final isEnabled = _status['enabled'] ?? false;
    final pqAvailable = _status['pq_available'] ?? false;

    if (!hasKeys) {
      return _buildStatusCard(
        icon: Icons.warning_amber,
        color: Colors.orange,
        title: 'Chiavi crittografia mancanti',
        subtitle: 'Genera le chiavi per abilitare la crittografia post-quantistica',
        onTap: _generateKeys,
      );
    }

    if (!isEnabled) {
      return _buildStatusCard(
        icon: Icons.lock_open,
        color: Colors.blue,
        title: 'Crittografia disabilitata',
        subtitle: 'La crittografia è disabilitata. I messaggi non saranno crittografati.',
        onTap: () => _toggleEncryption(true),
      );
    }

    if (!pqAvailable) {
      return _buildStatusCard(
        icon: Icons.shield,
        color: Colors.yellow,
        title: 'Crittografia legacy',
        subtitle: 'La crittografia post-quantistica non è disponibile.',
        onTap: null,
      );
    }

    return _buildStatusCard(
      icon: Icons.shield,
      color: Colors.green,
      title: 'Crittografia post-quantistica attiva',
      subtitle: 'I messaggi sono crittografati con tecnologia post-quantistica.',
      onTap: null,
    );
  }

  Widget _buildStatusCard({
    required IconData icon,
    required Color color,
    required String title,
    required String subtitle,
    VoidCallback? onTap,
  }) {
    return Card(
      margin: const EdgeInsets.symmetric(horizontal: 16, vertical: 8),
      child: InkWell(
        onTap: onTap,
        borderRadius: BorderRadius.circular(8),
        child: Padding(
          padding: const EdgeInsets.all(12),
          child: Row(
            children: [
              Icon(icon, color: color, size: 24),
              const SizedBox(width: 12),
              Expanded(
                child: Column(
                  crossAxisAlignment: CrossAxisAlignment.start,
                  children: [
                    Text(
                      title,
                      style: TextStyle(
                        fontWeight: FontWeight.bold,
                        color: color,
                        fontSize: 14,
                      ),
                    ),
                    const SizedBox(height: 4),
                    Text(
                      subtitle,
                      style: const TextStyle(fontSize: 12, color: Colors.grey),
                    ),
                  ],
                ),
              ),
              if (onTap != null)
                const Icon(Icons.arrow_forward_ios, size: 16, color: Colors.grey),
            ],
          ),
        ),
      ),
    );
  }

  Future<void> _generateKeys() async {
    try {
      await _encryptionManager.initialize();
      await _loadEncryptionStatus();
      if (!mounted) return;
      ScaffoldMessenger.of(context).showSnackBar(
        const SnackBar(
          content: Text('Chiavi crittografia generate con successo!'),
          backgroundColor: Colors.green,
        ),
      );
    } catch (e) {
      if (!mounted) return;
      ScaffoldMessenger.of(context).showSnackBar(
        SnackBar(
          content: Text('Errore nella generazione delle chiavi: $e'),
          backgroundColor: Colors.red,
        ),
      );
    }
  }

  Future<void> _toggleEncryption(bool enable) async {
    try {
      await _encryptionManager.setEncryptionEnabled(enable);
      await _loadEncryptionStatus();
      if (!mounted) return;
      ScaffoldMessenger.of(context).showSnackBar(
        SnackBar(
          content: Text(enable ? 'Crittografia abilitata' : 'Crittografia disabilitata'),
          backgroundColor: enable ? Colors.green : Colors.orange,
        ),
      );
    } catch (e) {
      if (!mounted) return;
      ScaffoldMessenger.of(context).showSnackBar(
        SnackBar(
          content: Text('Errore nell\'impostazione della crittografia: $e'),
          backgroundColor: Colors.red,
        ),
      );
    }
  }
}

/// Widget to display detailed encryption information
class EncryptionInfoWidget extends StatelessWidget {
  const EncryptionInfoWidget({super.key});

  @override
  Widget build(BuildContext context) {
    return Card(
      margin: const EdgeInsets.symmetric(horizontal: 16, vertical: 8),
      child: Padding(
        padding: const EdgeInsets.all(16),
        child: Column(
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            const Text(
              'Informazioni Crittografia',
              style: TextStyle(fontWeight: FontWeight.bold, fontSize: 16),
            ),
            const SizedBox(height: 8),
            const Text(
              'Fedi3 utilizza la crittografia post-quantistica per proteggere i tuoi messaggi di chat.',
              style: TextStyle(fontSize: 14, color: Colors.grey),
            ),
            const SizedBox(height: 12),
            _buildInfoRow(Icons.shield, 'Kyber768 KEM', 'Scambio chiavi post-quantistico'),
            _buildInfoRow(Icons.lock, 'AES-256-GCM', 'Cifratura messaggi'),
            _buildInfoRow(Icons.vpn_key, 'HKDF', 'Derivazione chiavi'),
            _buildInfoRow(Icons.refresh, 'Forward Secrecy', 'Segretezza forward'),
            const SizedBox(height: 12),
            const Text(
              'La crittografia è abilitata per impostazione predefinita e non può essere disattivata per le chat di gruppo.',
              style: TextStyle(fontSize: 12, color: Colors.grey),
            ),
          ],
        ),
      ),
    );
  }

  Widget _buildInfoRow(IconData icon, String title, String subtitle) {
    return Padding(
      padding: const EdgeInsets.symmetric(vertical: 4),
      child: Row(
        children: [
          Icon(icon, size: 16, color: Colors.blue),
          const SizedBox(width: 8),
          Column(
            crossAxisAlignment: CrossAxisAlignment.start,
            children: [
              Text(title, style: const TextStyle(fontSize: 13, fontWeight: FontWeight.bold)),
              Text(subtitle, style: const TextStyle(fontSize: 12, color: Colors.grey)),
            ],
          ),
        ],
      ),
    );
  }
}
