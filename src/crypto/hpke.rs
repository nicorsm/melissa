// Wire
// Copyright (C) 2019 Wire Swiss GmbH
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.
//
// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the
// GNU General Public License for more details.
//
// You should have received a copy of the GNU General Public License
// along with this program. If not, see http://www.gnu.org/licenses/.

use codec::*;
use crypto::aesgcm::*;
use crypto::hkdf;
use keys::*;
use std::*;

pub type HpkeKemError = AesError;

pub const NK_AES_GCM_128: usize = 16;
pub const NN_AES_GCM_128: usize = 12;

pub const NK_AES_GCM_256: usize = 32;
pub const NN_AES_GCM_256: usize = 12;

pub const NK_CHACHA20POLY1305: usize = 32;
pub const NN_CHACHA20POLY1305: usize = 12;

fn setup_core_x25519_aes_128(
    mode: u8,
    secret: &[u8],
    kem_context: &[u8],
    info: &[u8],
) -> (Vec<u8>, Vec<u8>) {
    let ciphersuite = HpkeCipherSuite::X25519Sha256Aes128gcm as u16;
    let nk = NK_AES_GCM_128;
    let nn = NN_AES_GCM_128;

    let hpke_context = HpkeContext {
        ciphersuite,
        mode,
        kem_context: kem_context.to_vec(),
        info: info.to_vec(),
    };

    let mut context_buffer: Vec<u8> = Vec::new();

    hpke_context.encode(&mut context_buffer);

    // key = Expand(secret, "hpke key" + context, Nk)

    let label_str: &str = "hpke key";
    let mut label: Vec<u8> = Vec::new();
    label.extend_from_slice(label_str.as_bytes());
    label.append(&mut context_buffer.clone());

    let key = hkdf::expand(
        hkdf::Prk::from_slice(&secret).unwrap(),
        hkdf::Info(&label),
        nk,
    );

    // nonce = Expand(secret, "hpke nonce" + context, Nn)

    let label_str: &str = "hpke nonce";
    let mut label: Vec<u8> = Vec::new();
    label.extend_from_slice(label_str.as_bytes());
    label.append(&mut context_buffer.clone());

    let nonce = hkdf::expand(
        hkdf::Prk::from_slice(&secret).unwrap(),
        hkdf::Info(&label),
        nn,
    );

    (key, nonce)
}

// SetupBase(pkR, zz, enc, info):

fn setup_base_x25519_aes_128(
    pkr: &X25519PublicKey,
    zz: &[u8],
    enc: &[u8],
    info: &[u8],
) -> (Vec<u8>, Vec<u8>) {
    let mode = HpkeMode::Base as u8;
    let mut kem_context: Vec<u8> = Vec::new();
    kem_context.extend_from_slice(&enc);
    kem_context.extend_from_slice(&pkr.to_slice());

    let salt = [0u8; 32];
    let secret = &hkdf::extract(hkdf::Salt(&salt), hkdf::Input(&zz)).0;

    setup_core_x25519_aes_128(mode, secret, &kem_context, info)
}

// def Encap(pkR):
//     skE, pkE = GenerateKeyPair()
//     zz = DH(skE, pkR)
//     enc = Marshal(pkE)
//     return zz, enc

// fn encap_x25519(pkr: &X25519PublicKey) -> (Vec<u8>, Vec<u8>) {}

pub enum HpkeMode {
    Base = 0x00,
    Psk = 0x01,
    Auth = 0x02,
}

pub enum HpkeCipherSuite {
    P256Sha256Aes128gcm = 0x0001,
    P521Sha512Aes256gcm = 0x0002,
    X25519Sha256Aes128gcm = 0x003,
    X448Sha512Aes256gcm = 0x0004,
}

pub struct HpkeContext {
    ciphersuite: u16,
    mode: u8,
    kem_context: Vec<u8>,
    info: Vec<u8>,
}

impl Codec for HpkeContext {
    fn encode(&self, buffer: &mut Vec<u8>) {
        self.ciphersuite.encode(buffer);
        self.mode.encode(buffer);
        encode_vec_u8(buffer, &self.kem_context);
        encode_vec_u8(buffer, &self.info);
    }
    fn decode(cursor: &mut Cursor) -> Result<Self, CodecError> {
        let ciphersuite = u16::decode(cursor)?;
        let mode = u8::decode(cursor)?;
        let kem_context = decode_vec_u8(cursor)?;
        let info = decode_vec_u8(cursor)?;
        Ok(HpkeContext {
            ciphersuite,
            mode,
            kem_context,
            info,
        })
    }
}

pub struct HpkeCiphertext {
    pub ephemeral_public_key: X25519PublicKey,
    pub content: Vec<u8>,
}

impl HpkeCiphertext {
    fn enc_x25519_aes(
        public_key: &X25519PublicKey,
        enc: &[u8],
        ephemeral_key_pair: &X25519KeyPair,
    ) -> Result<HpkeCiphertext, HpkeKemError> {
        let zz = ephemeral_key_pair
            .private_key
            .shared_secret(public_key)
            .unwrap();
        let (key, nonce) = setup_base_x25519_aes_128(public_key, &zz, enc, &[]);
        let content = aes_128_seal(
            enc,
            &Aes128Key::from_slice(&key),
            &Nonce::from_slice(&nonce),
        )?;
        Ok(HpkeCiphertext {
            ephemeral_public_key: ephemeral_key_pair.public_key,
            content,
        })
    }
    pub fn encrypt(
        public_key: &X25519PublicKey,
        enc: &[u8],
    ) -> Result<HpkeCiphertext, HpkeKemError> {
        let key_pair = X25519KeyPair::new_random();
        HpkeCiphertext::enc_x25519_aes(public_key, enc, &key_pair)
    }
    pub fn encrypt_with_ephemeral(
        public_key: &X25519PublicKey,
        enc: &[u8],
        key_pair: &X25519KeyPair,
    ) -> Result<HpkeCiphertext, HpkeKemError> {
        HpkeCiphertext::enc_x25519_aes(public_key, enc, &key_pair)
    }
}