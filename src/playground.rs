// Playground URL encoder.
//
// Encodes source code into a URL hash fragment compatible with the playground's
// decodeSource(). Pipeline: UTF-8 → deflate-raw → base62.

use std::io::Write;
use flate2::Compression;
use flate2::write::DeflateEncoder;

const BASE62: &[u8; 62] = b"0123456789abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ";

/// Encode source code into a base62 string suitable for a playground URL hash.
pub fn encode_source(src: &str) -> String {
  let bytes = src.as_bytes();

  // Deflate-raw compress
  let mut encoder = DeflateEncoder::new(Vec::new(), Compression::default());
  encoder.write_all(bytes).expect("deflate write");
  let compressed = encoder.finish().expect("deflate finish");

  if compressed.is_empty() {
    return String::from("0");
  }

  // Count leading zero bytes (preserved as leading '0' digits)
  let leading_zeros = compressed.iter().take_while(|&&b| b == 0).count();

  // Treat compressed bytes as big-endian unsigned integer
  let mut n = to_bigint(compressed.as_slice());

  // Convert to base62
  let mut out = Vec::new();
  if is_zero(&n) {
    out.push(BASE62[0]);
  } else {
    while !is_zero(&n) {
      let (quotient, remainder) = div_mod_62(&n);
      out.push(BASE62[remainder]);
      n = quotient;
    }
  }
  out.reverse();

  // Prepend leading '0' digits for preserved zero bytes
  let mut result = String::with_capacity(leading_zeros + out.len());
  for _ in 0..leading_zeros {
    result.push('0');
  }
  for &b in &out {
    result.push(b as char);
  }
  result
}

// Big-endian bytes → Vec<u32> limbs (least-significant first) for big integer arithmetic.
fn to_bigint(bytes: &[u8]) -> Vec<u32> {
  let mut limbs: Vec<u32> = vec![0];
  for &b in bytes {
    // limbs = limbs * 256 + b
    let mut carry = b as u64;
    for limb in limbs.iter_mut() {
      let v = (*limb as u64) * 256 + carry;
      *limb = v as u32;
      carry = v >> 32;
    }
    if carry > 0 {
      limbs.push(carry as u32);
    }
  }
  limbs
}

// Divide big integer (Vec<u32> limbs, LSB first) by 62, return (quotient, remainder).
fn div_mod_62(limbs: &[u32]) -> (Vec<u32>, usize) {
  let mut result = vec![0u32; limbs.len()];
  let mut remainder: u64 = 0;
  for i in (0..limbs.len()).rev() {
    let cur = (remainder << 32) | limbs[i] as u64;
    result[i] = (cur / 62) as u32;
    remainder = cur % 62;
  }
  // Trim leading zero limbs
  while result.len() > 1 && *result.last().unwrap() == 0 {
    result.pop();
  }
  (result, remainder as usize)
}

fn is_zero(limbs: &[u32]) -> bool {
  limbs.iter().all(|&l| l == 0)
}
