use ethers_core::types::{U256, U512};
use num_bigint::{
    BigInt,
    Sign::{NoSign, Plus},
};
use zokrates_field::{Bn128Field, Field};

pub trait ToU256 {
    fn to_u256(&self) -> U256;
}

impl ToU256 for Bn128Field {
    fn to_u256(&self) -> U256 {
        let mut bytes: [u8; 32] = self.to_byte_vector().try_into().unwrap();
        bytes.reverse();
        bytes.into()
    }
}

impl ToU256 for BigInt {
    fn to_u256(&self) -> U256 {
        let bytes = self.to_bytes_be();
        assert!(bytes.0 == NoSign || bytes.0 == Plus);
        bytes.1.as_slice().try_into().unwrap()
    }
}

pub trait ToBn128Field {
    fn to_bn128_field(&self) -> Bn128Field;
}

impl ToBn128Field for U256 {
    fn to_bn128_field(&self) -> Bn128Field {
        let mut n_bytes = vec![0; 32];
        self.to_little_endian(&mut n_bytes);
        Bn128Field::from_byte_vector(n_bytes)
    }
}

pub trait ToBigInt {
    fn to_big_int(&self) -> BigInt;
}

impl ToBigInt for U256 {
    fn to_big_int(&self) -> BigInt {
        let mut bytes = vec![0; 32];
        self.to_big_endian(&mut bytes);
        BigInt::from_bytes_be(Plus, bytes.as_slice())
    }
}

pub trait ToBabyJubjubSignature {
    fn to_babyjubjub_signature(&self) -> babyjubjub_rs::Signature;
}

impl ToBabyJubjubSignature for String {
    fn to_babyjubjub_signature(&self) -> babyjubjub_rs::Signature {
        U512::from_dec_str(self).unwrap().to_babyjubjub_signature()
    }
}

impl ToBabyJubjubSignature for U512 {
    fn to_babyjubjub_signature(&self) -> babyjubjub_rs::Signature {
        let mut bytes = vec![0; 64];
        self.to_little_endian(&mut bytes);
        babyjubjub_rs::decompress_signature(bytes.as_slice().try_into().unwrap()).unwrap()
    }
}

pub trait ToBabyJubjubPoint {
    fn to_babyjubjub_point(&self) -> babyjubjub_rs::Point;
}

impl ToBabyJubjubPoint for U256 {
    fn to_babyjubjub_point(&self) -> babyjubjub_rs::Point {
        let mut bytes = vec![0; 32];
        self.to_big_endian(&mut bytes);
        babyjubjub_rs::decompress_point(bytes.try_into().unwrap()).unwrap()
    }
}

pub struct PrivateKey(pub babyjubjub_rs::PrivateKey);

impl From<String> for PrivateKey {
    fn from(key: String) -> Self {
        U256::from_dec_str(&key).unwrap().into()
    }
}

impl From<U256> for PrivateKey {
    fn from(key: U256) -> Self {
        let mut bytes = vec![0; 32];
        key.to_big_endian(&mut bytes);
        Self(babyjubjub_rs::PrivateKey::import(bytes).unwrap())
    }
}
