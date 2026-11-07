#!/usr/bin/env python3
import base64
import importlib.util
import json
from pathlib import Path
import unittest
from cryptography.exceptions import InvalidSignature
from cryptography.hazmat.primitives import hashes, serialization
from cryptography.hazmat.primitives.asymmetric import ed25519, rsa, padding

spec=importlib.util.spec_from_file_location('receipt_verifier',Path(__file__).resolve().parents[1]/'verify_forward_receipt.py')
module=importlib.util.module_from_spec(spec);spec.loader.exec_module(module)

class Receipts(unittest.TestCase):
    def setUp(self):
        self.relay=ed25519.Ed25519PrivateKey.generate()
        self.public=base64.b64encode(self.relay.public_key().public_bytes(serialization.Encoding.Raw,serialization.PublicFormat.Raw)).decode()
    def sign(self,event):
        payload=json.dumps({'schema':'fedi3-forward-receipt/v1','sequence':1,'previous_hash':'0'*64,'relay':'https://fixture.invalid','event':event},separators=(',',':')).encode()
        return {'algorithm':'ed25519','public_key_b64':self.public,'payload_b64':base64.b64encode(payload).decode(),'signature_b64':base64.b64encode(self.relay.sign(payload)).decode()}
    def test_relay_signature_pin_and_tampering(self):
        envelope=self.sign({'outcome':'spooled','status':202})
        self.assertEqual(module.verify(envelope,self.public)['outcome'],'spooled')
        with self.assertRaises(ValueError):module.verify(envelope,'different-key')
        envelope['payload_b64']=base64.b64encode(b'changed').decode()
        with self.assertRaises(InvalidSignature):module.verify(envelope,self.public)
    def test_core_proof_requires_independent_key_and_matching_binding(self):
        core=rsa.generate_private_key(public_exponent=65537,key_size=2048)
        pem=core.public_key().public_bytes(serialization.Encoding.PEM,serialization.PublicFormat.SubjectPublicKeyInfo)
        body=json.dumps({'schema':'fedi3-core-forward-ack/v1','actor':'https://actor.invalid/users/a','request_digest':'a'*64,'status':202}).encode()
        ack={'algorithm':'rsa-sha256','payload_b64':base64.b64encode(body).decode(),'signature_b64':base64.b64encode(core.sign(body,padding.PKCS1v15(),hashes.SHA256())).decode(),'key_id':'https://actor.invalid/users/a#main-key'}
        event={'outcome':'core_acknowledged','status':202,'request_digest':'a'*64,'core_ack':ack}
        envelope=self.sign(event)
        self.assertTrue(module.verify(envelope,self.public,pem)['core_signature_verified'])
        with self.assertRaises(ValueError):module.verify(envelope,self.public)
        event['request_digest']='b'*64
        with self.assertRaises(ValueError):module.verify(self.sign(event),self.public,pem)

if __name__=='__main__':unittest.main()
