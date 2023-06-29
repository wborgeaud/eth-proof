use anyhow::Result;
use eth_trie_utils::nibbles::Nibbles;
use eth_trie_utils::partial_trie::{HashedPartialTrie, Node, PartialTrie};
use ethers::prelude::*;
use ethers::utils::keccak256;
use ethers::utils::rlp;

pub fn insert_proof(
    trie: &mut HashedPartialTrie,
    address: Address,
    proof: Vec<Bytes>,
) -> Result<()> {
    let key = keccak256(address.0);
    let mut nibbles = Nibbles::from_bytes_be(&key)?;
    let mut current_prefix = Nibbles {
        count: 0,
        packed: U256::zero(),
    };
    for p in proof {
        let a = rlp::decode_list::<Vec<u8>>(&p);
        // dbg!(&a);
        dbg!(current_prefix, a.len());
        match a.len() {
            17 => {
                let nibble = nibbles.pop_next_nibble_front();
                for i in 0..16 {
                    if i == nibble {
                        continue;
                    }
                    let mut new_prefix = current_prefix;
                    new_prefix.push_nibble_back(i);
                    dbg!(
                        new_prefix,
                        trie.get(new_prefix),
                        trie.whatsup(&mut new_prefix.clone())
                    );
                    if !a[i as usize].is_empty() && !trie.whatsup(&mut new_prefix.clone()) {
                        let hash = H256::from_slice(&a[i as usize]);
                        trie.insert(new_prefix, hash);
                    }
                    dbg!(new_prefix, trie.get(new_prefix));
                }
                current_prefix.push_nibble_back(nibble);
            }
            2 => match a[0][0] >> 4 {
                0 => {
                    let ext_prefix = &a[0][1..];
                    for &byte in ext_prefix {
                        let b = byte >> 4;
                        let nibble = nibbles.pop_next_nibble_front();
                        assert_eq!(b, nibble);
                        current_prefix.push_nibble_back(b);
                        let b = byte & 0xf;
                        let nibble = nibbles.pop_next_nibble_front();
                        assert_eq!(b, nibble);
                        current_prefix.push_nibble_back(b);
                    }
                }
                1 => {
                    let b = a[0][0] & 0xf;
                    let nibble = nibbles.pop_next_nibble_front();
                    assert_eq!(b, nibble);
                    current_prefix.push_nibble_back(b);
                    let ext_prefix = &a[0][1..];
                    for &byte in ext_prefix {
                        let b = byte >> 4;
                        let nibble = nibbles.pop_next_nibble_front();
                        assert_eq!(b, nibble);
                        current_prefix.push_nibble_back(b);
                        let b = byte & 0xf;
                        let nibble = nibbles.pop_next_nibble_front();
                        assert_eq!(b, nibble);
                        current_prefix.push_nibble_back(b);
                    }
                }
                2 => {
                    let leaf_prefix = &a[0][1..];
                    for &byte in leaf_prefix {
                        let b = byte >> 4;
                        let nibble = nibbles.pop_next_nibble_front();
                        assert_eq!(b, nibble);
                        current_prefix.push_nibble_back(b);
                        let b = byte & 0xf;
                        let nibble = nibbles.pop_next_nibble_front();
                        assert_eq!(b, nibble);
                        current_prefix.push_nibble_back(b);
                    }
                    assert_eq!(current_prefix, Nibbles::from_bytes_be(&key)?);
                    trie.insert(current_prefix, a[1].clone());
                }
                3 => {
                    let b = a[0][0] & 0xf;
                    let nibble = nibbles.pop_next_nibble_front();
                    assert_eq!(b, nibble);
                    current_prefix.push_nibble_back(b);
                    let leaf_prefix = &a[0][1..];
                    for &byte in leaf_prefix {
                        let b = byte >> 4;
                        let nibble = nibbles.pop_next_nibble_front();
                        assert_eq!(b, nibble);
                        current_prefix.push_nibble_back(b);
                        let b = byte & 0xf;
                        let nibble = nibbles.pop_next_nibble_front();
                        assert_eq!(b, nibble);
                        current_prefix.push_nibble_back(b);
                    }
                    assert_eq!(current_prefix, Nibbles::from_bytes_be(&key)?);
                    trie.insert(current_prefix, a[1].clone());
                }
                _ => panic!("wtf?"),
            },
            _ => panic!("wtf?"),
        }
    }
    dbg!(trie.hash());

    Ok(())
}
