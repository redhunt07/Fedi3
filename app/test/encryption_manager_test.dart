/*
 * SPDX-FileCopyrightText: 2026 RedHunt07 - FEDI3 Project
 * SPDX-License-Identifier: AGPL-3.0-only
 */

import 'package:flutter/services.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:shared_preferences/shared_preferences.dart';
import 'package:fedi3/services/encryption_manager.dart';
import 'package:fedi3/model/chat_models.dart';

void main() {
  TestWidgetsFlutterBinding.ensureInitialized();
  const storageChannel = MethodChannel('plugins.it_nomads.com/flutter_secure_storage');
  final storageData = <String, String>{};
  final binaryMessenger = TestDefaultBinaryMessengerBinding.instance.defaultBinaryMessenger;
  binaryMessenger.setMockMethodCallHandler(storageChannel, (call) async {
    final args = call.arguments as Map<dynamic, dynamic>? ?? {};
    final key = args['key'] as String?;
    switch (call.method) {
      case 'read':
        if (key == null) return null;
        return storageData[key];
      case 'write':
        if (key == null) return null;
        final value = args['value'] as String?;
        if (value != null) {
          storageData[key] = value;
        }
        return null;
      case 'containsKey':
        if (key == null) return false;
        return storageData.containsKey(key);
      case 'delete':
        if (key != null) {
          storageData.remove(key);
        }
        return null;
      case 'deleteAll':
        storageData.clear();
        return null;
      default:
        return null;
    }
  });
  group('EncryptionManager', () {
    late EncryptionManager encryptionManager;

    setUp(() async {
      storageData.clear();
      // Initialize SharedPreferences for testing
      SharedPreferences.setMockInitialValues({});
      
      encryptionManager = EncryptionManager();
      await encryptionManager.initialize();
    });

    test('should initialize and generate keys', () async {
      final hasKeys = await encryptionManager.hasKeys();
      expect(hasKeys, isTrue);
    });

    test('should get user public key', () async {
      final publicKey = await encryptionManager.getUserPublicKey();
      expect(publicKey, isNotEmpty);
    });

    test('should get user private key', () async {
      final privateKey = await encryptionManager.getUserPrivateKey();
      expect(privateKey, isNotEmpty);
    });

    test('should check encryption enabled by default', () async {
      final isEnabled = await encryptionManager.isEncryptionEnabled();
      expect(isEnabled, isTrue);
    });

    test('should enable/disable encryption', () async {
      // Disable encryption
      await encryptionManager.setEncryptionEnabled(false);
      var isEnabled = await encryptionManager.isEncryptionEnabled();
      expect(isEnabled, isFalse);

      // Enable encryption
      await encryptionManager.setEncryptionEnabled(true);
      isEnabled = await encryptionManager.isEncryptionEnabled();
      expect(isEnabled, isTrue);
    });

    test('should encrypt and decrypt chat message', () async {
      final payload = ChatPayload(
        op: 'message',
        text: 'Hello, encrypted world!',
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

      const threadId = 'test-thread-123';
      final messageId = encryptionManager.generateMessageId();
      final recipientPublicKey = await encryptionManager.getUserPublicKey();

      // Encrypt message
      final encryptedEnvelope = await encryptionManager.encryptChatMessage(
        payload: payload,
        threadId: threadId,
        messageId: messageId,
        recipientPublicKey: recipientPublicKey,
      );

      expect(encryptedEnvelope, isNotEmpty);
      expect(encryptedEnvelope['thread_id'], threadId);
      expect(encryptedEnvelope['message_id'], messageId);

      // Decrypt message
      final decryptedPayload = await encryptionManager.decryptChatMessage(
        envelope: encryptedEnvelope,
      );

      expect(decryptedPayload.op, payload.op);
      expect(decryptedPayload.text, payload.text);
    });

    test('should handle unencrypted messages', () async {
      await encryptionManager.setEncryptionEnabled(false);

      final payload = ChatPayload(
        op: 'message',
        text: 'Unencrypted message',
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

      const threadId = 'test-thread';
      final messageId = encryptionManager.generateMessageId();
      final recipientPublicKey = await encryptionManager.getUserPublicKey();

      // Encrypt message (should be unencrypted)
      final encryptedEnvelope = await encryptionManager.encryptChatMessage(
        payload: payload,
        threadId: threadId,
        messageId: messageId,
        recipientPublicKey: recipientPublicKey,
      );

      expect(encryptedEnvelope['unencrypted'], isTrue);

      // Decrypt message
      final decryptedPayload = await encryptionManager.decryptChatMessage(
        envelope: encryptedEnvelope,
      );

      expect(decryptedPayload.op, payload.op);
      expect(decryptedPayload.text, payload.text);
    });

    test('should encrypt message for multiple recipients', () async {
      final payload = ChatPayload(
        op: 'message',
        text: 'Multi-recipient message',
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

      const threadId = 'test-thread';
      final messageId = encryptionManager.generateMessageId();
      final recipientPublicKeys = [
        await encryptionManager.getUserPublicKey(),
        await encryptionManager.getUserPublicKey(), // Same key for simplicity
      ];

      final encryptedMessages = await encryptionManager.encryptMessageForRecipients(
        payload: payload,
        threadId: threadId,
        messageId: messageId,
        recipientPublicKeys: recipientPublicKeys,
      );

      expect(encryptedMessages, isNotEmpty);
      expect(encryptedMessages.length, equals(recipientPublicKeys.length));
    });

    test('should encrypt thread metadata', () async {
      const threadId = 'test-thread-456';
      const title = 'Test Encrypted Thread';
      final members = ['user1@example.com', 'user2@example.com'];

      final encryptedMetadata = await encryptionManager.encryptThreadMetadata(
        threadId: threadId,
        title: title,
        members: members,
      );

      expect(encryptedMetadata, isNotEmpty);
      expect(encryptedMetadata['thread_id'], threadId);
    });

    test('should handle unencrypted thread metadata', () async {
      await encryptionManager.setEncryptionEnabled(false);

      const threadId = 'test-thread';
      const title = 'Unencrypted Thread';
      final members = ['user1@example.com'];

      final encryptedMetadata = await encryptionManager.encryptThreadMetadata(
        threadId: threadId,
        title: title,
        members: members,
      );

      expect(encryptedMetadata['unencrypted'], isTrue);
      expect(encryptedMetadata['title'], title);
      expect(encryptedMetadata['members'], equals(members));
    });

    test('should verify message integrity', () async {
      final envelope = {
        'thread_id': 'test-thread',
        'message_id': 'test-message',
        'payload': {'op': 'message', 'text': 'test'},
      };

      final isValid = encryptionManager.verifyMessageIntegrity(envelope, 'dummy-signature');
      expect(isValid, isTrue);
    });

    test('should generate message ID', () {
      final messageId = encryptionManager.generateMessageId();
      expect(messageId, isNotEmpty);
      expect(messageId.length, greaterThan(10));
    });

    test('should export and import keys', () async {
      // Export keys
      final exportedKeys = await encryptionManager.exportKeys();
      expect(exportedKeys['public_key'], isNotEmpty);
      expect(exportedKeys['private_key'], isNotEmpty);

      // Clear keys
      await encryptionManager.clearKeys();
      var hasKeys = await encryptionManager.hasKeys();
      expect(hasKeys, isFalse);

      // Import keys
      await encryptionManager.importKeys(exportedKeys);
      hasKeys = await encryptionManager.hasKeys();
      expect(hasKeys, isTrue);
    });

    test('should get encryption status', () async {
      final status = await encryptionManager.getEncryptionStatus();
      
      expect(status, isMap);
      expect(status['has_keys'], isTrue);
      expect(status['enabled'], isTrue);
      expect(status['public_key_available'], isTrue);
      expect(status['pq_available'], isTrue);
    });

    test('should encrypt and decrypt simple text message', () async {
      const text = 'Hello, simple text!';
      const threadId = 'test-thread';
      final recipientPublicKey = await encryptionManager.getUserPublicKey();

      // Encrypt text message
      final encryptedEnvelope = await encryptionManager.encryptTextMessage(
        text: text,
        threadId: threadId,
        recipientPublicKey: recipientPublicKey,
      );

      expect(encryptedEnvelope, isNotEmpty);

      // Decrypt text message
      final decryptedText = await encryptionManager.decryptTextMessage(encryptedEnvelope);
      expect(decryptedText, text);
    });

    test('should handle decryption errors gracefully', () async {
      final invalidEnvelope = {
        'thread_id': 'test-thread',
        'message_id': 'test-message',
        'ciphertext_b64': 'invalid-base64',
        'nonce_b64': 'invalid-nonce',
      };

      expect(
        () => encryptionManager.decryptChatMessage(envelope: invalidEnvelope),
        throwsException,
      );
    });
  });
}
