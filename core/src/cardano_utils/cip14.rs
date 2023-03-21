use crate::{AssetName, PolicyId, TokenId};
use anyhow::{anyhow, Context as _, Result};

use cryptoxide::hashing::blake2b::Blake2b;

fn fingerprint_hash(policy: &PolicyId, name: &AssetName) -> anyhow::Result<[u8; 20]> {
    let mut buf = vec![0u8; 28 + name.as_ref().len() / 2];
    hex::decode_to_slice(policy.as_ref(), &mut buf[..28])
        .with_context(|| anyhow!("Failed to decode PolicyId: {policy}"))?;
    hex::decode_to_slice(name.as_ref(), &mut buf[28..])
        .with_context(|| anyhow!("Failed to decode AssetName: {name}"))?;

    let b2b = Blake2b::<{ 20 * 8 }>::new().update(&buf);

    let mut out = [0; 20];
    b2b.finalize_at(&mut out);

    Ok(out)
}

pub fn fingerprint(policy: &PolicyId, name: &AssetName) -> Result<TokenId> {
    fingerprint_hash(policy, name).map(|hash| TokenId::new(format!("{:0<64}", hex::encode(hash))))
}

#[cfg(test)]
mod tests {
    use super::*;
    use bech32::ToBase32;

    const HRP: &str = "asset";

    pub fn fingerprint_bech32(policy: &PolicyId, name: &AssetName) -> Result<String> {
        let bytes = fingerprint_hash(policy, name)?;
        bech32::encode(HRP, bytes.to_base32(), bech32::Variant::Bech32)
            .context("Couldn't compute bech32 asset fingerprint")
    }

    struct TestVector {
        policy_id: PolicyId,
        asset_name: AssetName,
        asset_fingerprint: &'static str,
    }

    const TESTS: &[TestVector] = &[
        TestVector {
            policy_id: PolicyId::new_static(
                "7eae28af2208be856f7a119668ae52a49b73725e326dc16579dcc373",
            ),
            asset_name: AssetName::new_static(""),
            asset_fingerprint: "asset1rjklcrnsdzqp65wjgrg55sy9723kw09mlgvlc3",
        },
        TestVector {
            policy_id: PolicyId::new_static(
                "7eae28af2208be856f7a119668ae52a49b73725e326dc16579dcc37e",
            ),
            asset_name: AssetName::new_static(""),
            asset_fingerprint: "asset1nl0puwxmhas8fawxp8nx4e2q3wekg969n2auw3",
        },
        TestVector {
            policy_id: PolicyId::new_static(
                "1e349c9bdea19fd6c147626a5260bc44b71635f398b67c59881df209",
            ),
            asset_name: AssetName::new_static(""),
            asset_fingerprint: "asset1uyuxku60yqe57nusqzjx38aan3f2wq6s93f6ea",
        },
        TestVector {
            policy_id: PolicyId::new_static(
                "7eae28af2208be856f7a119668ae52a49b73725e326dc16579dcc373",
            ),
            asset_name: AssetName::new_static("504154415445"),
            asset_fingerprint: "asset13n25uv0yaf5kus35fm2k86cqy60z58d9xmde92",
        },
        TestVector {
            policy_id: PolicyId::new_static(
                "1e349c9bdea19fd6c147626a5260bc44b71635f398b67c59881df209",
            ),
            asset_name: AssetName::new_static("504154415445"),
            asset_fingerprint: "asset1hv4p5tv2a837mzqrst04d0dcptdjmluqvdx9k3",
        },
        TestVector {
            policy_id: PolicyId::new_static(
                "1e349c9bdea19fd6c147626a5260bc44b71635f398b67c59881df209",
            ),
            asset_name: AssetName::new_static(
                "7eae28af2208be856f7a119668ae52a49b73725e326dc16579dcc373",
            ),
            asset_fingerprint: "asset1aqrdypg669jgazruv5ah07nuyqe0wxjhe2el6f",
        },
        TestVector {
            policy_id: PolicyId::new_static(
                "7eae28af2208be856f7a119668ae52a49b73725e326dc16579dcc373",
            ),
            asset_name: AssetName::new_static(
                "1e349c9bdea19fd6c147626a5260bc44b71635f398b67c59881df209",
            ),
            asset_fingerprint: "asset17jd78wukhtrnmjh3fngzasxm8rck0l2r4hhyyt",
        },
        TestVector {
            policy_id: PolicyId::new_static(
                "7eae28af2208be856f7a119668ae52a49b73725e326dc16579dcc373",
            ),
            asset_name: AssetName::new_static(
                "0000000000000000000000000000000000000000000000000000000000000000",
            ),
            asset_fingerprint: "asset1pkpwyknlvul7az0xx8czhl60pyel45rpje4z8w",
        },
    ];

    #[test]
    fn test_vectors() {
        for (index, test) in TESTS.iter().enumerate() {
            let computed = fingerprint_bech32(&test.policy_id, &test.asset_name).unwrap();

            assert_eq!(
                computed, test.asset_fingerprint,
                "Failed to run the vector test {index}"
            )
        }
    }
}
