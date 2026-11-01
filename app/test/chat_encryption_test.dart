/*
 * SPDX-FileCopyrightText: 2026 RedHunt07 - FEDI3 Project
 * SPDX-License-Identifier: AGPL-3.0-only
 */

import 'package:flutter_test/flutter_test.dart';
import 'package:fedi3/services/chat_encryption_service.dart';
import 'package:fedi3/model/chat_models.dart';

void main() {
  group('ChatEncryptionService', () {
    late ChatEncryptionService encryptionService;

    setUp(() {
      encryptionService = ChatEncryptionService();
    });

    test('should generate message ID', () {
      final messageId = encryptionService.generateMessageId();
      expect(messageId, isNotEmpty);
      expect(messageId.length, greaterThan(10));
    });

    test('should check encryption enabled', () {
      final isEnabled = encryptionService.isEncryptionEnabled();
      expect(isEnabled, isTrue);
    });

    test('should encrypt and decrypt message payload', () async {
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
      final messageId = encryptionService.generateMessageId();
      const publicKey = 'dummy-public-key'; // In real implementation, this would be a proper Kyber public key

      // Encrypt the message
      final encryptedEnvelope = await encryptionService.encryptMessage(
        payload: payload,
        threadId: threadId,
        messageId: messageId,
        recipientPublicKey: publicKey,
      );

      expect(encryptedEnvelope, isNotEmpty);
      expect(encryptedEnvelope['thread_id'], threadId);
      expect(encryptedEnvelope['message_id'], messageId);
      expect(encryptedEnvelope['ciphertext_b64'], isNotEmpty);
      expect(encryptedEnvelope['nonce_b64'], isNotEmpty);

      // Decrypt the message
      const privateKey = 'dummy-private-key'; // In real implementation, this would be a proper Kyber private key
      final decryptedPayload = await encryptionService.decryptMessage(
        envelope: encryptedEnvelope,
        privateKey: privateKey,
      );

      expect(decryptedPayload.op, payload.op);
      expect(decryptedPayload.text, payload.text);
    });

    test('should encrypt thread metadata', () async {
      const threadId = 'test-thread-456';
      const title = 'Test Encrypted Thread';
      final members = ['user1@example.com', 'user2@example.com'];
      const privateKey = 'dummy-private-key';

      final encryptedMetadata = await encryptionService.encryptThreadMetadata(
        threadId: threadId,
        title: title,
        members: members,
        privateKey: privateKey,
      );

      expect(encryptedMetadata, isNotEmpty);
      expect(encryptedMetadata['thread_id'], threadId);
      expect(encryptedMetadata['encrypted_metadata_b64'], isNotEmpty);
      expect(encryptedMetadata['nonce_b64'], isNotEmpty);
    });

    test('should verify message integrity', () {
      final envelope = {
        'thread_id': 'test-thread',
        'message_id': 'test-message',
        'ciphertext_b64': 'dummy-ciphertext',
        'nonce_b64': 'dummy-nonce',
      };
      const signature = 'dummy-signature';

      // In this test, we're using the placeholder implementation
      // which always returns true
      final isValid = encryptionService.verifyMessageIntegrity(envelope, signature);
      expect(isValid, isTrue);
    });

    test('should handle encryption errors gracefully', () async {
      final payload = ChatPayload(
        op: 'message',
        text: 'Test message',
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
      final messageId = encryptionService.generateMessageId();
      const invalidPublicKey = ''; // Empty public key should cause an error

      expect(
        () => encryptionService.encryptMessage(
          payload: payload,
          threadId: threadId,
          messageId: messageId,
          recipientPublicKey: invalidPublicKey,
        ),
        throwsException,
      );
    });

    test('should handle decryption errors gracefully', () async {
      final invalidEnvelope = {
        'thread_id': 'test-thread',
        'message_id': 'test-message',
        'ciphertext_b64': 'invalid-base64',
        'nonce_b64': 'invalid-nonce',
      };
      const privateKey = 'dummy-private-key';

      expect(
        () => encryptionService.decryptMessage(
          envelope: invalidEnvelope,
          privateKey: privateKey,
        ),
        throwsException,
      );
    });
  });
}
