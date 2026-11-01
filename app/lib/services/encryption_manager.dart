/*
 * SPDX-FileCopyrightText: 2026 RedHunt07 - FEDI3 Project
 * SPDX-License-Identifier: AGPL-3.0-only
 */


import 'package:flutter_secure_storage/flutter_secure_storage.dart';

import '../model/chat_models.dart';
import 'chat_encryption_service.dart';
import 'post_quantum_crypto.dart';

/// Encryption Manager for Fedi3
/// 
/// This service manages encryption keys, provides encryption/decryption
/// functionality, and handles key exchange for chat messages.
class EncryptionManager {
  static final EncryptionManager _instance = EncryptionManager._internal();

  factory EncryptionManager() => _instance;

  EncryptionManager._internal();

  static const String _userPublicKeyKey = 'user_public_key';
  static const String _userPrivateKeyKey = 'user_private_key';
  static const String _encryptionEnabledKey = 'encryption_enabled';

  final ChatEncryptionService _chatEncryptionService = ChatEncryptionService();
  final PostQuantumCryptoService _pqService = PostQuantumCryptoService();

  /// Initialize encryption manager
  Future<void> initialize() async {
    // Check if user has existing keys
    const storage = FlutterSecureStorage();
    final hasKeys = await storage.containsKey(key: _userPublicKeyKey) && 
                   await storage.containsKey(key: _userPrivateKeyKey);
    
    if (!hasKeys) {
      // Generate new key pair
      await _generateAndStoreKeyPair();
    }
  }

  /// Generate and store a new Kyber768 key pair
  Future<void> _generateAndStoreKeyPair() async {
    const storage = FlutterSecureStorage();
    final keyPair = _pqService.generateKyberKeyPair();
    
    await storage.write(key: _userPublicKeyKey, value: keyPair.publicKey);
    await storage.write(key: _userPrivateKeyKey, value: keyPair.secretKey);
  }

  /// Get the user's public key
  Future<String> getUserPublicKey() async {
    const storage = FlutterSecureStorage();
    return await storage.read(key: _userPublicKeyKey) ?? '';
  }

  /// Get the user's private key
  Future<String> getUserPrivateKey() async {
    const storage = FlutterSecureStorage();
    return await storage.read(key: _userPrivateKeyKey) ?? '';
  }

  /// Check if encryption is enabled
  Future<bool> isEncryptionEnabled() async {
    const storage = FlutterSecureStorage();
    final value = await storage.read(key: _encryptionEnabledKey);
    return value != 'false';
  }

  /// Enable or disable encryption
  Future<void> setEncryptionEnabled(bool enabled) async {
    const storage = FlutterSecureStorage();
    await storage.write(key: _encryptionEnabledKey, value: enabled ? 'true' : 'false');
  }

  /// Encrypt a chat message
  Future<Map<String, dynamic>> encryptChatMessage({
    required ChatPayload payload,
    required String threadId,
    required String messageId,
    required String recipientPublicKey,
  }) async {
    if (!await isEncryptionEnabled()) {
      // If encryption is disabled, return the payload as-is
      final unencryptedEnvelope = {
        'v': 1,
        'thread_id': threadId,
        'message_id': messageId,
        'payload': payload.toJson(),
        'unencrypted': true,
      };

      return unencryptedEnvelope;
    }

    return _chatEncryptionService.encryptMessage(
      payload: payload,
      threadId: threadId,
      messageId: messageId,
      recipientPublicKey: recipientPublicKey,
    );
  }

  /// Decrypt a chat message
  Future<ChatPayload> decryptChatMessage({
    required Map<String, dynamic> envelope,
  }) async {
    if (envelope['unencrypted'] == true) {
      // Message is not encrypted
      final payloadMap = envelope['payload'] as Map<String, dynamic>;
      return ChatPayload.fromJson(payloadMap);
    }

    final privateKey = await getUserPrivateKey();
    if (privateKey.isEmpty) {
      throw Exception('No private key available for decryption');
    }

    return _chatEncryptionService.decryptMessage(
      envelope: envelope,
      privateKey: privateKey,
    );
  }

  /// Encrypt a message for multiple recipients
  Future<List<Map<String, dynamic>>> encryptMessageForRecipients({
    required ChatPayload payload,
    required String threadId,
    required String messageId,
    required List<String> recipientPublicKeys,
  }) async {
    if (!await isEncryptionEnabled()) {
      // If encryption is disabled, return the payload as-is for each recipient
      final unencryptedEnvelope = {
        'v': 1,
        'thread_id': threadId,
        'message_id': messageId,
        'payload': payload.toJson(),
        'unencrypted': true,
      };
      return List.filled(recipientPublicKeys.length, unencryptedEnvelope);
    }

    return _chatEncryptionService.encryptMessageForRecipients(
      payload: payload,
      threadId: threadId,
      messageId: messageId,
      recipientPublicKeys: recipientPublicKeys,
    );
  }

  /// Decrypt a message from multiple possible keys
  Future<ChatPayload?> decryptMessageFromMultipleKeys({
    required Map<String, dynamic> envelope,
    required List<String> privateKeys,
  }) async {
    if (envelope['unencrypted'] == true) {
      // Message is not encrypted
      final payloadMap = envelope['payload'] as Map<String, dynamic>;
      return ChatPayload.fromJson(payloadMap);
    }

    return _chatEncryptionService.decryptMessageFromMultipleKeys(
      envelope: envelope,
      privateKeys: privateKeys,
    );
  }

  /// Encrypt thread metadata
  Future<Map<String, dynamic>> encryptThreadMetadata({
    required String threadId,
    required String title,
    required List<String> members,
  }) async {
    if (!await isEncryptionEnabled()) {
      // If encryption is disabled, return the metadata as-is
      return {
        'thread_id': threadId,
        'title': title,
        'members': members,
        'unencrypted': true,
      };
    }

    final privateKey = await getUserPrivateKey();
    if (privateKey.isEmpty) {
      throw Exception('No private key available for encryption');
    }

    return _chatEncryptionService.encryptThreadMetadata(
      threadId: threadId,
      title: title,
      members: members,
      privateKey: privateKey,
    );
  }

  /// Verify message integrity
  bool verifyMessageIntegrity(Map<String, dynamic> envelope, String signature) {
    if (envelope['unencrypted'] == true) {
      // For unencrypted messages, we can't verify cryptographic integrity
      // but we can check basic structure
      return envelope.containsKey('thread_id') && 
             envelope.containsKey('message_id') &&
             envelope.containsKey('payload');
    }

    return _chatEncryptionService.verifyMessageIntegrity(envelope, signature);
  }

  /// Generate a new message ID
  String generateMessageId() {
    return _chatEncryptionService.generateMessageId();
  }

  /// Export user keys for backup
  Future<Map<String, String>> exportKeys() async {
    final publicKey = await getUserPublicKey();
    final privateKey = await getUserPrivateKey();
    
    return {
      'public_key': publicKey,
      'private_key': privateKey,
    };
  }

  /// Import user keys from backup
  Future<void> importKeys(Map<String, String> keys) async {
    const storage = FlutterSecureStorage();
    await storage.write(key: _userPublicKeyKey, value: keys['public_key'] ?? '');
    await storage.write(key: _userPrivateKeyKey, value: keys['private_key'] ?? '');
  }

  /// Clear all encryption keys (for account reset)
  Future<void> clearKeys() async {
    const storage = FlutterSecureStorage();
    await storage.delete(key: _userPublicKeyKey);
    await storage.delete(key: _userPrivateKeyKey);
    await storage.delete(key: _encryptionEnabledKey);
  }

  /// Check if the user has encryption keys
  Future<bool> hasKeys() async {
    final publicKey = await getUserPublicKey();
    final privateKey = await getUserPrivateKey();
    return publicKey.isNotEmpty && privateKey.isNotEmpty;
  }

  /// Get encryption status summary
  Future<Map<String, dynamic>> getEncryptionStatus() async {
    final hasKeys = await this.hasKeys();
    final isEnabled = await isEncryptionEnabled();
    final publicKey = await getUserPublicKey();
    
    return {
      'has_keys': hasKeys,
      'enabled': isEnabled,
      'public_key_available': publicKey.isNotEmpty,
      'pq_available': _pqService.isPostQuantumAvailable(),
    };
  }

  /// Encrypt a simple text message
  Future<Map<String, dynamic>> encryptTextMessage({
    required String text,
    required String threadId,
    required String recipientPublicKey,
  }) async {
    final payload = ChatPayload(
      op: 'message',
      text: text,
      replyTo: null,
      messageId: null,
      status: null,
      threadId: null,
      attachments: null,
      action: null,
      targets: null,
      members: null,
      title: null,
      reaction: null,
    );

    final messageId = generateMessageId();
    return encryptChatMessage(
      payload: payload,
      threadId: threadId,
      messageId: messageId,
      recipientPublicKey: recipientPublicKey,
    );
  }

  /// Decrypt a simple text message
  Future<String> decryptTextMessage(Map<String, dynamic> envelope) async {
    final payload = await decryptChatMessage(envelope: envelope);
    return payload.text ?? '';
  }
}
