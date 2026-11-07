# Public compatibility fixture — not production keys

The private key here is intentionally public, generated only for these tests.
Never use it for an account or real encryption.

`private.pem`, `public.pem` and `vectors.json` were produced by the original
`rsa` 0.9.10 implementation before replacing it. The vectors cover PKCS#8/SPKI
PEM, the profile DID, deterministic PKCS#1 v1.5 SHA256 signatures and randomized
RSA-OAEP SHA256/MGF1-SHA256 ciphertext with the empty label.

Tests compare public-key bytes and signatures exactly, decrypt the old ciphertext
with the new provider, reject tampering and verify that loading an existing
profile never changes its private-key file or DID.

`encrypted_message.json` is a complete encrypted v1 relay/P2P request generated
by the old test oracle (RSA 0.9.10 + AES-256-GCM). Its AES key and nonce are
public fixture values; production still generates fresh random values per message.
The test decrypts it with the new provider and checks the original message body.
