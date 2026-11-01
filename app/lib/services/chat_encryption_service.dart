/*
 * SPDX-FileCopyrightText: 2026 RedHunt07 - FEDI3 Project
 * SPDX-License-Identifier: AGPL-3.0-only
 */

import 'dart:convert';
import 'dart:math';

import 'package:flutter/foundation.dart';

import '../model/chat_models.dart';
import 'post_quantum_crypto.dart';

class ChatEncryptionService {
  static final ChatEncryptionService _instance = ChatEncryptionService._internal();

  factory ChatEncryptionService() => _instance;

  ChatEncryptionService._internal();

  final PostQuantumCryptoService _pqService = PostQuantumCryptoService();

  // Post-quantum parameters
  static const int aesKeySize = 32; // 256-bit AES
  static const int nonceSize = 12;  // 96-bit nonce
  static const String _defaultKemAlgorithm = 'kyber768';

  // Encrypt chat message for a specific recipient using post-quantum encryption
  Future<Map<String, dynamic>> encryptMessage({
    required ChatPayload payload,
    required String threadId,
    required String messageId,
    required String recipientPublicKey, // Base64 encoded
  }) async {
    try {
      if (recipientPublicKey.trim().isEmpty) {
        throw ArgumentError('Recipient public key is required');
      }
      // Use post-quantum encryption for the message
      final payloadJson = jsonEncode(payload.toJson());
      final envelope = await _pqService.createEncryptedEnvelope(
        plaintext: payloadJson,
        recipientPublicKey: recipientPublicKey,
        threadId: threadId,
        messageId: messageId,
      );

      // Create the final envelope structure
      final finalEnvelope = {
        'v': envelope.version,
        'thread_id': envelope.threadId,
        'message_id': envelope.messageId,
        'sender_actor': '', // Will be filled by core
        'sender_device': '', // Will be filled by core
        'sender_peer_id': '', // Will be filled by core
        'created_at_ms': DateTime.now().millisecondsSinceEpoch,
        'kem_alg': envelope.kemAlgorithm,
        'kem_ciphertext_b64': envelope.kemCiphertext,
        'kem_key_id': '', // Will be filled by core
        'nonce_b64': envelope.nonce,
        'ciphertext_b64': envelope.ciphertext,
        'signature_b64': '', // Will be filled by core
      };

      return finalEnvelope;
    } catch (e) {
      throw Exception('Post-quantum encryption failed: $e');
    }
  }

  // Decrypt chat message envelope using post-quantum encryption
  Future<ChatPayload> decryptMessage({
    required Map<String, dynamic> envelope,
    required String privateKey, // Base64 encoded
  }) async {
    try {
      // Create envelope object from the map
      final encryptedEnvelope = EncryptedMessageEnvelope(
        version: envelope['v'] as int,
        threadId: envelope['thread_id'] as String,
        messageId: envelope['message_id'] as String,
        kemAlgorithm: envelope['kem_alg'] as String,
        kemCiphertext: envelope['kem_ciphertext_b64'] as String,
        nonce: envelope['nonce_b64'] as String,
        ciphertext: envelope['ciphertext_b64'] as String,
        tag: envelope['tag'] ?? envelope['signature_b64'] ?? '', // Use tag or signature as tag
        createdAt: envelope['created_at'] ?? DateTime.now().toUtc().toIso8601String(),
      );

      // Use post-quantum decryption
      final payloadJson = await _pqService.decryptEnvelope(
        envelope: encryptedEnvelope,
        secretKey: privateKey,
      );

      // Parse payload
      final payloadMap = jsonDecode(payloadJson) as Map<String, dynamic>;
      return ChatPayload.fromJson(payloadMap);
    } catch (e) {
      throw Exception('Post-quantum decryption failed: $e');
    }
  }

  // Encrypt chat thread metadata
  Future<Map<String, dynamic>> encryptThreadMetadata({
    required String threadId,
    required String title,
    required List<String> members,
    required String privateKey, // Our private key
  }) async {
    try {
      final metadata = {
        'thread_id': threadId,
        'title': title,
        'members': members,
        'created_at': DateTime.now().toIso8601String(),
      };

      final metadataJson = jsonEncode(metadata);
      final keyPair = _pqService.generateKyberKeyPair();
      
      // Use the private key to encrypt the metadata
      final envelope = await _pqService.createEncryptedEnvelope(
        plaintext: metadataJson,
        recipientPublicKey: keyPair.publicKey, // Use generated public key
        threadId: threadId,
        messageId: 'metadata-${DateTime.now().millisecondsSinceEpoch}',
      );

      return {
        'thread_id': threadId,
        'encrypted_metadata_b64': envelope.ciphertext,
        'nonce_b64': envelope.nonce,
        'key_encrypted_for_members': {}, // Will be populated with member-specific encryption
        'public_key': keyPair.publicKey, // Store public key for decryption
      };
    } catch (e) {
      throw Exception('Thread metadata encryption failed: $e');
    }
  }

  // Verify message integrity
  bool verifyMessageIntegrity(Map<String, dynamic> envelope, String signature) {
    try {
      final version = envelope['v'] is int
          ? envelope['v'] as int
          : int.tryParse(envelope['v']?.toString() ?? '') ?? 1;
      final threadId = envelope['thread_id']?.toString() ?? '';
      final messageId = envelope['message_id']?.toString() ?? '';
      final kemAlgorithm = envelope['kem_alg']?.toString() ?? _defaultKemAlgorithm;
      final kemCiphertext = envelope['kem_ciphertext_b64']?.toString() ?? '';
      final nonce = envelope['nonce_b64']?.toString() ?? '';
      final ciphertext = envelope['ciphertext_b64']?.toString() ?? '';
      final tag = envelope['tag']?.toString() ?? envelope['signature_b64']?.toString() ?? '';
      final createdAt = envelope['created_at']?.toString() ?? DateTime.now().toUtc().toIso8601String();

      // Create envelope object for verification
      final encryptedEnvelope = EncryptedMessageEnvelope(
        version: version,
        threadId: threadId,
        messageId: messageId,
        kemAlgorithm: kemAlgorithm,
        kemCiphertext: kemCiphertext,
        nonce: nonce,
        ciphertext: ciphertext,
        tag: tag,
        createdAt: createdAt,
      );

      // Use post-quantum verification
      return _pqService.verifyMessageIntegrity(encryptedEnvelope, signature);
    } catch (e) {
      return false;
    }
  }

  // Generate message ID
  String generateMessageId() {
    final random = Random.secure();
    final bytes = Uint8List(16);
    for (var i = 0; i < bytes.length; i++) {
      bytes[i] = random.nextInt(256);
    }
    return base64UrlEncode(bytes).replaceAll('=', '').replaceAll('+', '-').replaceAll('/', '_');
  }

  // Check if encryption is enabled
  bool isEncryptionEnabled() {
    // Check if post-quantum encryption is available
    return _pqService.isPostQuantumAvailable();
  }

  // Generate Kyber768 key pair for the user
  KyberKeyPair generateUserKeyPair() {
    return _pqService.generateKyberKeyPair();
  }

  // Get the user's public key for sharing with contacts
  String getUserPublicKey() {
    // In a real implementation, this would retrieve the stored public key
    // For now, generate a new one
    final keyPair = _pqService.generateKyberKeyPair();
    return keyPair.publicKey;
  }

  // Encrypt message for multiple recipients
  Future<List<Map<String, dynamic>>> encryptMessageForRecipients({
    required ChatPayload payload,
    required String threadId,
    required String messageId,
    required List<String> recipientPublicKeys,
  }) async {
    final encryptedMessages = <Map<String, dynamic>>[];
    
    for (final publicKey in recipientPublicKeys) {
      try {
        final encryptedMessage = await encryptMessage(
          payload: payload,
          threadId: threadId,
          messageId: messageId,
          recipientPublicKey: publicKey,
        );
        encryptedMessages.add(encryptedMessage);
      } catch (e) {
        // Log error but continue with other recipients
        debugPrint('Failed to encrypt message for recipient: $e');
      }
    }
    
    return encryptedMessages;
  }

  // Decrypt message from multiple possible private keys
  Future<ChatPayload?> decryptMessageFromMultipleKeys({
    required Map<String, dynamic> envelope,
    required List<String> privateKeys,
  }) async {
    for (final privateKey in privateKeys) {
      try {
        final decryptedPayload = await decryptMessage(
          envelope: envelope,
          privateKey: privateKey,
        );
        return decryptedPayload;
      } catch (e) {
        // Try next key
        continue;
      }
    }
    return null; // No key could decrypt the message
  }
}
