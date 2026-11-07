#!/usr/bin/env python3
"""Verify exact signed bytes. Obtain/pin keys independently of the receipt issuer."""
import argparse
import base64
import hashlib
import json
from pathlib import Path


def verify(envelope, relay_key, core_key_pem=None):
    from cryptography.hazmat.primitives.asymmetric.ed25519 import Ed25519PublicKey
    from cryptography.hazmat.primitives.asymmetric import padding
    from cryptography.hazmat.primitives import hashes, serialization
    if envelope.get('algorithm') != 'ed25519' or envelope.get('public_key_b64') != relay_key:
        raise ValueError('unexpected relay signing key or algorithm')
    payload = base64.b64decode(envelope['payload_b64'], validate=True)
    if len(payload) > 65536:
        raise ValueError('receipt payload too large')
    Ed25519PublicKey.from_public_bytes(base64.b64decode(relay_key, validate=True)).verify(
        base64.b64decode(envelope['signature_b64'], validate=True), payload)
    value = json.loads(payload)
    if value.get('schema') != 'fedi3-forward-receipt/v1':
        raise ValueError('unexpected receipt schema')
    event = value['event']
    core_verified = False
    if event['outcome'] in ('core_acknowledged', 'core_rejected'):
        if core_key_pem is None:
            raise ValueError('independently pinned core public key required for core acknowledgment')
        key = serialization.load_pem_public_key(core_key_pem)
        ack = event['core_ack']
        if ack.get('algorithm') != 'rsa-sha256':
            raise ValueError('unexpected core algorithm')
        body = base64.b64decode(ack['payload_b64'], validate=True)
        key.verify(base64.b64decode(ack['signature_b64'], validate=True), body, padding.PKCS1v15(), hashes.SHA256())
        receipt = json.loads(body)
        if (receipt.get('schema') != 'fedi3-core-forward-ack/v1'
                or receipt['request_digest'] != event['request_digest']
                or receipt['status'] != event['status']
                or ack['key_id'] != receipt['actor'] + '#main-key'):
            raise ValueError('core acknowledgment does not match forward')
        core_verified = True
    commitment=hashlib.sha256(payload).hexdigest()
    link=json.dumps(('fedi3-forward-log-link/v1',value['sequence'],value['previous_hash'],commitment),separators=(',',':')).encode()
    return {'sequence': value['sequence'], 'previous_hash': value['previous_hash'],
            'hash': hashlib.sha256(link).hexdigest(), 'commitment': commitment, 'outcome': event['outcome'],
            'status': event['status'], 'relay_signature_verified': True,
            'core_signature_verified': core_verified}


def verify_log(document, relay_key):
    from cryptography.hazmat.primitives.asymmetric.ed25519 import Ed25519PublicKey
    checkpoint=document['checkpoint']
    if checkpoint.get('algorithm')!='ed25519' or checkpoint.get('public_key_b64')!=relay_key:
        raise ValueError('unexpected checkpoint signing key')
    raw=base64.b64decode(checkpoint['payload_b64'],validate=True)
    Ed25519PublicKey.from_public_bytes(base64.b64decode(relay_key,validate=True)).verify(
        base64.b64decode(checkpoint['signature_b64'],validate=True),raw)
    value=json.loads(raw)
    if value.get('schema')!='fedi3-forward-log-head/v1' or value['head']!=document['head']:
        raise ValueError('checkpoint/head mismatch')
    head=value['head'];sequence=head['anchor_sequence'];previous=head['anchor_hash']
    for entry in document['items']:
        sequence+=1
        if entry['sequence']!=sequence or entry['previous_hash']!=previous:
            raise ValueError('log gap or reordered entry')
        link=json.dumps(('fedi3-forward-log-link/v1',sequence,previous,entry['commitment']),separators=(',',':')).encode()
        previous=hashlib.sha256(link).hexdigest()
        if entry['hash']!=previous:raise ValueError('invalid log commitment link')
    if sequence!=head['sequence'] or previous!=head['hash']:
        raise ValueError('incomplete log: collect pages from signed anchor through signed head')
    return {'checkpoint_signature_verified':True,'chain_verified':True,'sequence':sequence,'hash':previous}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('receipt', type=Path, help='signed envelope JSON, not HTTP-header base64')
    parser.add_argument('--relay-key', required=True, help='independently pinned Ed25519 public key, base64')
    parser.add_argument('--public-log',action='store_true',help='verify a complete public log page/document instead of a private receipt')
    parser.add_argument('--core-key', type=Path, help='independently pinned actor RSA public key PEM')
    args = parser.parse_args()
    document=json.loads(args.receipt.read_text())
    result = verify_log(document,args.relay_key) if args.public_log else verify(document, args.relay_key,
                    args.core_key.read_bytes() if args.core_key else None)
    print(json.dumps(result, indent=2))


if __name__ == '__main__':
    main()
