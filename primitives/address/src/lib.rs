#![cfg_attr(not(feature = "std"), no_std)]

extern crate alloc;

use alloc::{string::String, vec::Vec};

const CHARSET: &[u8; 32] = b"qpzry9x8gf2tvdw0s3jn54khce6mua7l";
const GENERATORS: [u32; 5] = [0x3b6a57b2, 0x26508e6d, 0x1ea119fa, 0x3d4233dd, 0x2a1462b3];
const BECH32M: u32 = 0x2bc830a3;
const VERSION: u8 = 0;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Network {
	Mainnet,
	Testnet,
}

impl Network {
	pub fn hrp(self) -> &'static str {
		match self {
			Self::Mainnet => "mfcx",
			Self::Testnet => "tmfcx",
		}
	}

	fn from_hrp(hrp: &str) -> Result<Self, AddressError> {
		match hrp {
			"mfcx" => Ok(Self::Mainnet),
			"tmfcx" => Ok(Self::Testnet),
			_ => Err(AddressError::InvalidHrp),
		}
	}
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AddressError {
	InvalidAccountId,
	InvalidCharacter,
	InvalidCase,
	InvalidLength,
	InvalidHrp,
	NetworkMismatch,
	InvalidChecksum,
	InvalidPadding,
	InvalidVersion,
}

impl core::fmt::Display for AddressError {
	fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
		let message = match self {
			Self::InvalidAccountId => "invalid AccountId32",
			Self::InvalidCharacter => "invalid Bech32 character",
			Self::InvalidCase => "MFCoin addresses must be lowercase",
			Self::InvalidLength => "incorrect address or payload length",
			Self::InvalidHrp => "expected mfcx or tmfcx",
			Self::NetworkMismatch => "address belongs to another network",
			Self::InvalidChecksum => "Bech32m checksum mismatch",
			Self::InvalidPadding => "non-zero or incomplete Bech32m padding",
			Self::InvalidVersion => "unsupported address version",
		};
		formatter.write_str(message)
	}
}

#[cfg(feature = "std")]
impl std::error::Error for AddressError {}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DecodedAddress {
	pub network: Network,
	pub account_id: [u8; 32],
}

fn hrp_expand(hrp: &str) -> Vec<u8> {
	let mut expanded = Vec::with_capacity(hrp.len() * 2 + 1);
	expanded.extend(hrp.bytes().map(|character| character >> 5));
	expanded.push(0);
	expanded.extend(hrp.bytes().map(|character| character & 31));
	expanded
}

fn polymod(values: &[u8]) -> u32 {
	let mut checksum = 1u32;
	for value in values {
		let top = checksum >> 25;
		checksum = ((checksum & 0x1ffffff) << 5) ^ u32::from(*value);
		for (index, generator) in GENERATORS.iter().enumerate() {
			if (top >> index) & 1 == 1 {
				checksum ^= generator;
			}
		}
	}
	checksum
}

fn convert_bits(
	values: &[u8],
	from_bits: u32,
	to_bits: u32,
	pad: bool,
) -> Result<Vec<u8>, AddressError> {
	let mut accumulator = 0u32;
	let mut bits = 0u32;
	let mut output = Vec::new();
	let max_value = (1u32 << to_bits) - 1;
	let max_accumulator = (1u32 << (from_bits + to_bits - 1)) - 1;
	for value in values {
		if u32::from(*value) >> from_bits != 0 {
			return Err(AddressError::InvalidCharacter);
		}
		accumulator = ((accumulator << from_bits) | u32::from(*value)) & max_accumulator;
		bits += from_bits;
		while bits >= to_bits {
			bits -= to_bits;
			output.push(((accumulator >> bits) & max_value) as u8);
		}
	}
	if pad {
		if bits > 0 {
			output.push(((accumulator << (to_bits - bits)) & max_value) as u8);
		}
	} else if bits >= from_bits || ((accumulator << (to_bits - bits)) & max_value) != 0 {
		return Err(AddressError::InvalidPadding);
	}
	Ok(output)
}

fn encode_payload(network: Network, payload: &[u8]) -> Result<String, AddressError> {
	let mut data = convert_bits(payload, 8, 5, true)?;
	let mut checksum_input = hrp_expand(network.hrp());
	checksum_input.extend_from_slice(&data);
	checksum_input.extend_from_slice(&[0, 0, 0, 0, 0, 0]);
	let checksum = polymod(&checksum_input) ^ BECH32M;
	for index in 0..6 {
		data.push(((checksum >> (5 * (5 - index))) & 31) as u8);
	}
	let mut address = String::with_capacity(network.hrp().len() + 60);
	address.push_str(network.hrp());
	address.push('1');
	for value in data {
		address.push(CHARSET[usize::from(value)] as char);
	}
	Ok(address)
}

pub fn encode_address(network: Network, account_id: &[u8; 32]) -> Result<String, AddressError> {
	let mut payload = Vec::with_capacity(33);
	payload.push(VERSION);
	payload.extend_from_slice(account_id);
	encode_payload(network, &payload)
}

pub fn decode_address(address: &str) -> Result<DecodedAddress, AddressError> {
	for character in address.chars() {
		if !character.is_ascii() || character.is_ascii_control() || character == ' ' {
			return Err(AddressError::InvalidCharacter);
		}
		if character.is_ascii_uppercase() {
			return Err(AddressError::InvalidCase);
		}
	}
	let separator = address.rfind('1').ok_or(AddressError::InvalidHrp)?;
	let network = Network::from_hrp(&address[..separator])?;
	let encoded = &address[separator + 1..];
	let mut data = Vec::with_capacity(encoded.len());
	for character in encoded.bytes() {
		let value = CHARSET
			.iter()
			.position(|candidate| *candidate == character)
			.ok_or(AddressError::InvalidCharacter)?;
		data.push(value as u8);
	}
	if address.len() != network.hrp().len() + 60 || data.len() < 7 {
		return Err(AddressError::InvalidLength);
	}
	let mut checksum_input = hrp_expand(network.hrp());
	checksum_input.extend_from_slice(&data);
	if polymod(&checksum_input) != BECH32M {
		return Err(AddressError::InvalidChecksum);
	}
	let payload = convert_bits(&data[..data.len() - 6], 5, 8, false)?;
	if payload.len() != 33 {
		return Err(AddressError::InvalidLength);
	}
	if payload[0] != VERSION {
		return Err(AddressError::InvalidVersion);
	}
	let account_id = payload[1..]
		.try_into()
		.map_err(|_| AddressError::InvalidAccountId)?;
	Ok(DecodedAddress {
		network,
		account_id,
	})
}

pub fn validate_address(expected: Network, address: &str) -> Result<[u8; 32], AddressError> {
	let decoded = decode_address(address)?;
	if decoded.network != expected {
		return Err(AddressError::NetworkMismatch);
	}
	Ok(decoded.account_id)
}

#[cfg(test)]
mod tests {
	use super::*;
	use serde_json::Value;

	fn account_id(hex: &str) -> [u8; 32] {
		let hex = hex.trim_start_matches("0x");
		let mut account = [0u8; 32];
		for (index, byte) in account.iter_mut().enumerate() {
			*byte = u8::from_str_radix(&hex[index * 2..index * 2 + 2], 16).expect("valid hex");
		}
		account
	}

	#[test]
	fn canonical_vectors_round_trip() {
		let vectors: Value =
			serde_json::from_str(include_str!("../../../packages/address/test-vectors.json"))
				.expect("valid vectors");
		for vector in vectors["valid"].as_array().expect("valid array") {
			let network = Network::from_hrp(vector["hrp"].as_str().unwrap()).unwrap();
			let expected_account = account_id(vector["publicKey"].as_str().unwrap());
			let address = vector["address"].as_str().unwrap();
			assert_eq!(
				encode_address(network, &expected_account),
				Ok(address.into())
			);
			assert_eq!(
				decode_address(address),
				Ok(DecodedAddress {
					network,
					account_id: expected_account
				})
			);
			assert_eq!(validate_address(network, address), Ok(expected_account));
			assert_eq!(address.len(), network.hrp().len() + 60);
		}
	}

	#[test]
	fn network_mismatch_is_rejected() {
		let address = encode_address(Network::Mainnet, &[7; 32]).unwrap();
		assert_eq!(
			validate_address(Network::Testnet, &address),
			Err(AddressError::NetworkMismatch)
		);
	}

	#[test]
	fn checksum_changes_are_rejected() {
		let address = encode_address(Network::Mainnet, &[7; 32]).unwrap();
		let replacement = if address.ends_with('q') { 'p' } else { 'q' };
		let mut changed = address.clone();
		changed.pop();
		changed.push(replacement);
		assert_eq!(decode_address(&changed), Err(AddressError::InvalidChecksum));
		let mut changed = address.into_bytes();
		changed[12] = if changed[12] == b'q' { b'p' } else { b'q' };
		assert_eq!(
			decode_address(core::str::from_utf8(&changed).unwrap()),
			Err(AddressError::InvalidChecksum)
		);
	}

	#[test]
	fn case_hrp_and_length_are_rejected() {
		let address = encode_address(Network::Mainnet, &[7; 32]).unwrap();
		assert_eq!(
			decode_address(&address.to_uppercase()),
			Err(AddressError::InvalidCase)
		);
		assert_eq!(
			decode_address(&address.replacen("mfcx", "mf0x", 1)),
			Err(AddressError::InvalidHrp)
		);
		assert_eq!(
			decode_address(&address[..address.len() - 1]),
			Err(AddressError::InvalidLength)
		);
		assert_eq!(
			decode_address(&format!("{address}q")),
			Err(AddressError::InvalidLength)
		);
	}

	#[test]
	fn forbidden_and_non_ascii_characters_are_rejected() {
		assert!(
			decode_address("mfcx1q2w3e4r5t6y7u8i9o0p0a8s7d6f5g4h3j2k1l0z9x8c7v6b5n4m").is_err()
		);
		let address = encode_address(Network::Mainnet, &[7; 32]).unwrap();
		for invalid in ['b', 'i', 'o', 'б'] {
			let changed = format!("{}{}{}", &address[..10], invalid, &address[11..]);
			assert_eq!(
				decode_address(&changed),
				Err(AddressError::InvalidCharacter)
			);
		}
	}

	#[test]
	fn invalid_version_and_padding_are_rejected() {
		let address = encode_payload(Network::Mainnet, &[1; 33]).unwrap();
		assert_eq!(decode_address(&address), Err(AddressError::InvalidVersion));
		let address = encode_payload(Network::Mainnet, &[0; 32]).unwrap();
		assert_eq!(decode_address(&address), Err(AddressError::InvalidLength));
		assert_eq!(
			convert_bits(&[31], 5, 8, false),
			Err(AddressError::InvalidPadding)
		);
		assert_eq!(
			convert_bits(&[1], 5, 8, false),
			Err(AddressError::InvalidPadding)
		);
	}
}
