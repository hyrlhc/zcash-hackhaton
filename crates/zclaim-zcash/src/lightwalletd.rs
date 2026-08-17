//! A client for the light wallet protocol, as served by `lightwalletd` and Zaino.
//!
//! This is where ZClaim stops being a self-contained demo. The verifier's set
//! of acceptable anchors is populated from here, which is what makes anchor
//! authentication mean anything: a root the chain never produced is not in the
//! window, so a proof against a tree the prover invented is refused.
//!
//! Only the two calls that matter are implemented — `GetLightdInfo` for the
//! chain tip, and `GetTreeState` for a historical note commitment tree. The
//! protobuf messages for those are small enough to declare by hand, which keeps
//! `protoc` out of the build.

use std::time::Duration;

use ff::PrimeField;
use incrementalmerkletree::frontier::{CommitmentTree, Frontier};
use orchard::{tree::MerkleHashOrchard, NOTE_COMMITMENT_TREE_DEPTH};
use pasta_curves::pallas;
use tonic::transport::{Channel, ClientTlsConfig};
use tonic_prost::ProstCodec;

use crate::{anchor::RootWindow, Error};
use zclaim_circuits::Pool;

/// The gRPC service every light wallet server exposes.
const SERVICE: &str = "/cash.z.wallet.sdk.rpc.CompactTxStreamer";

/// The public Zcash testnet endpoint, useful as a default for demos.
pub const TESTNET_ENDPOINT: &str = "https://testnet.zec.rocks:443";

// --- wire messages ----------------------------------------------------------
//
// Field numbers are from `service.proto` in the lightwalletd repository. Only
// the fields this crate reads are declared; prost ignores the rest.

#[derive(Clone, PartialEq, prost::Message)]
struct Empty {}

#[derive(Clone, PartialEq, prost::Message)]
struct BlockId {
    #[prost(uint64, tag = "1")]
    height: u64,
    #[prost(bytes = "vec", tag = "2")]
    hash: Vec<u8>,
}

#[derive(Clone, PartialEq, prost::Message)]
struct LightdInfoMessage {
    #[prost(string, tag = "1")]
    version: String,
    #[prost(string, tag = "4")]
    chain_name: String,
    #[prost(string, tag = "6")]
    consensus_branch_id: String,
    #[prost(uint64, tag = "7")]
    block_height: u64,
}

#[derive(Clone, PartialEq, prost::Message)]
struct TreeStateMessage {
    #[prost(string, tag = "1")]
    network: String,
    #[prost(uint64, tag = "2")]
    height: u64,
    #[prost(string, tag = "3")]
    hash: String,
    #[prost(uint32, tag = "4")]
    time: u32,
    #[prost(string, tag = "5")]
    sapling: String,
    #[prost(string, tag = "6")]
    orchard: String,
}

// --- public types -----------------------------------------------------------

/// What the server says about itself and the chain.
#[derive(Clone, Debug)]
pub struct ChainInfo {
    /// Server version string.
    pub server_version: String,
    /// `"main"` or `"test"`.
    pub chain: String,
    /// Height of the chain tip.
    pub tip_height: u32,
    /// Consensus branch id, hex. Identifies the active network upgrade.
    pub consensus_branch: String,
}

impl ChainInfo {
    /// Whether this server is following testnet.
    pub fn is_testnet(&self) -> bool {
        self.chain == "test"
    }
}

/// A note commitment tree as of one block.
#[derive(Clone, Debug)]
pub struct TreeState {
    /// Height the tree state was taken at.
    pub height: u32,
    /// Block hash at that height.
    pub block_hash: String,
    /// The Orchard commitment tree, deserialised.
    tree: CommitmentTree<MerkleHashOrchard, { NOTE_COMMITMENT_TREE_DEPTH as u8 }>,
}

impl TreeState {
    /// The anchor: the root of the Orchard note commitment tree at this height.
    ///
    /// This is the value a verifier authenticates a proof's anchor against.
    pub fn anchor(&self) -> pallas::Base {
        field_from(self.tree.root())
    }

    /// Number of note commitments in the tree at this height.
    pub fn size(&self) -> u64 {
        self.tree.size() as u64
    }

    /// The frontier a prover starts from when building a witness for a note
    /// appended after this block.
    ///
    /// Returns the left-hand siblings per level, plus the position the next
    /// commitment will occupy — exactly what
    /// [`crate::TreeWitnessBuilder::from_frontier`] expects.
    pub fn frontier(&self) -> ([pallas::Base; NOTE_COMMITMENT_TREE_DEPTH], u32) {
        let next_position = self.size() as u32;
        let mut ommers = [pallas::Base::zero(); NOTE_COMMITMENT_TREE_DEPTH];

        if let Some(frontier) = self.to_frontier() {
            // `ommers` from a frontier are listed from the lowest level that
            // has one, so they are placed by walking the set bits of the
            // position rather than by index.
            let mut supplied = frontier.value().map(|v| v.ommers().to_vec()).unwrap_or_default();
            supplied.reverse();

            for (level, slot) in ommers.iter_mut().enumerate() {
                *slot = if (next_position >> level) & 1 == 1 {
                    supplied
                        .pop()
                        .map(field_from)
                        .unwrap_or_else(|| empty_root(level))
                } else {
                    empty_root(level)
                };
            }
        } else {
            for (level, slot) in ommers.iter_mut().enumerate() {
                *slot = empty_root(level);
            }
        }

        (ommers, next_position)
    }

    fn to_frontier(&self) -> Option<Frontier<MerkleHashOrchard, { NOTE_COMMITMENT_TREE_DEPTH as u8 }>>
    {
        if self.tree.is_empty() {
            None
        } else {
            Some(self.tree.to_frontier())
        }
    }
}

/// A connection to a light wallet server.
///
/// Blocking on the outside, async underneath. The rest of ZClaim is synchronous
/// and a wallet or a CLI has no reason to become async just to fetch a root.
pub struct LightwalletClient {
    runtime: tokio::runtime::Runtime,
    channel: Channel,
}

impl LightwalletClient {
    /// Connects to `endpoint`, e.g. [`TESTNET_ENDPOINT`].
    pub fn connect(endpoint: &str) -> Result<Self, Error> {
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(1)
            .enable_all()
            .build()
            .map_err(|e| Error::Transport(e.to_string()))?;

        let channel = runtime.block_on(async {
            Channel::from_shared(endpoint.to_string())
                .map_err(|e| Error::Transport(e.to_string()))?
                .tls_config(ClientTlsConfig::new().with_native_roots())
                .map_err(|e| Error::Transport(e.to_string()))?
                .timeout(Duration::from_secs(30))
                .connect()
                .await
                .map_err(|e| Error::Transport(e.to_string()))
        })?;

        Ok(LightwalletClient { runtime, channel })
    }

    /// Asks the server where the chain is.
    pub fn chain_info(&self) -> Result<ChainInfo, Error> {
        let info: LightdInfoMessage = self.call("GetLightdInfo", Empty {})?;
        Ok(ChainInfo {
            server_version: info.version,
            chain: info.chain_name,
            tip_height: info.block_height as u32,
            consensus_branch: info.consensus_branch_id,
        })
    }

    /// Fetches the note commitment tree as of `height`.
    pub fn tree_state(&self, height: u32) -> Result<TreeState, Error> {
        let state: TreeStateMessage = self.call(
            "GetTreeState",
            BlockId {
                height: height as u64,
                hash: Vec::new(),
            },
        )?;

        Ok(TreeState {
            height: state.height as u32,
            block_hash: state.hash,
            tree: parse_commitment_tree(&state.orchard)?,
        })
    }

    /// Fills a verifier's window with real roots, newest `count` blocks.
    ///
    /// This is the call that ties a verifier to the chain. Everything it will
    /// later accept has to appear here first.
    pub fn fill_root_window(
        &self,
        window: &mut RootWindow,
        pool: Pool,
        count: u32,
    ) -> Result<u32, Error> {
        let tip = self.chain_info()?.tip_height;

        for height in (tip.saturating_sub(count.saturating_sub(1))..=tip).rev() {
            let state = self.tree_state(height)?;
            window.observe(state.anchor(), pool, state.height);
        }

        Ok(tip)
    }

    fn call<Req, Res>(&self, method: &str, request: Req) -> Result<Res, Error>
    where
        Req: prost::Message + 'static,
        Res: prost::Message + Default + 'static,
    {
        let path = format!("{SERVICE}/{method}");
        let channel = self.channel.clone();

        self.runtime.block_on(async move {
            let mut grpc = tonic::client::Grpc::new(channel);
            grpc.ready()
                .await
                .map_err(|e| Error::Transport(e.to_string()))?;

            let codec: ProstCodec<Req, Res> = ProstCodec::default();
            let path = tonic::codegen::http::uri::PathAndQuery::from_maybe_shared(path)
                .map_err(|e| Error::Transport(e.to_string()))?;

            grpc.unary(tonic::Request::new(request), path, codec)
                .await
                .map(|response| response.into_inner())
                .map_err(|status| Error::Rpc {
                    method: method.to_string(),
                    message: status.message().to_string(),
                })
        })
    }
}

/// Parses the hex-encoded commitment tree a light wallet server returns.
///
/// The encoding is Zcash's long-standing `CommitmentTree` serialisation:
/// an optional left node, an optional right node, then a `CompactSize`-prefixed
/// vector of optional parent nodes. Each optional node is a presence byte
/// followed, if present, by a 32-byte hash.
fn parse_commitment_tree(
    hex_encoded: &str,
) -> Result<CommitmentTree<MerkleHashOrchard, { NOTE_COMMITMENT_TREE_DEPTH as u8 }>, Error> {
    if hex_encoded.is_empty() {
        return Ok(CommitmentTree::empty());
    }

    let bytes = hex::decode(hex_encoded)
        .map_err(|e| Error::MalformedTree(format!("tree state is not hex: {e}")))?;
    let mut reader = Reader::new(&bytes);

    let left = reader.optional_node()?;
    let right = reader.optional_node()?;

    let count = reader.compact_size()?;
    let mut parents = Vec::with_capacity(count.min(64));
    for _ in 0..count {
        parents.push(reader.optional_node()?);
    }

    CommitmentTree::from_parts(left, right, parents)
        .map_err(|_| Error::MalformedTree("tree is deeper than the Orchard tree".into()))
}

struct Reader<'a> {
    bytes: &'a [u8],
    at: usize,
}

impl<'a> Reader<'a> {
    fn new(bytes: &'a [u8]) -> Self {
        Reader { bytes, at: 0 }
    }

    fn take(&mut self, n: usize) -> Result<&'a [u8], Error> {
        let end = self.at.checked_add(n).ok_or_else(overflowed)?;
        let slice = self.bytes.get(self.at..end).ok_or_else(overflowed)?;
        self.at = end;
        Ok(slice)
    }

    fn byte(&mut self) -> Result<u8, Error> {
        Ok(self.take(1)?[0])
    }

    fn optional_node(&mut self) -> Result<Option<MerkleHashOrchard>, Error> {
        match self.byte()? {
            0 => Ok(None),
            1 => {
                let raw: [u8; 32] = self.take(32)?.try_into().expect("took exactly 32 bytes");
                Option::from(MerkleHashOrchard::from_bytes(&raw))
                    .map(Some)
                    .ok_or_else(|| Error::MalformedTree("node is not a valid hash".into()))
            }
            other => Err(Error::MalformedTree(format!(
                "expected an option tag, got {other}"
            ))),
        }
    }

    /// Bitcoin-style `CompactSize`.
    fn compact_size(&mut self) -> Result<usize, Error> {
        let n = match self.byte()? {
            n @ 0..=0xFC => n as u64,
            0xFD => u16::from_le_bytes(self.take(2)?.try_into().unwrap()) as u64,
            0xFE => u32::from_le_bytes(self.take(4)?.try_into().unwrap()) as u64,
            _ => u64::from_le_bytes(self.take(8)?.try_into().unwrap()),
        };

        usize::try_from(n).map_err(|_| Error::MalformedTree("length does not fit".into()))
    }
}

fn overflowed() -> Error {
    Error::MalformedTree("tree state ended mid-node".into())
}

fn field_from(node: MerkleHashOrchard) -> pallas::Base {
    pallas::Base::from_repr(node.to_bytes()).expect("a tree node is always a field element")
}

fn empty_root(level: usize) -> pallas::Base {
    use incrementalmerkletree::{Hashable, Level};
    field_from(MerkleHashOrchard::empty_root(Level::from(level as u8)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_empty_tree_state_parses() {
        let tree = parse_commitment_tree("").unwrap();
        assert!(tree.is_empty());
    }

    /// A tree with one leaf: left present, right absent, no parents.
    #[test]
    fn a_single_leaf_tree_parses_and_hashes() {
        let leaf = [0x11u8; 32];
        let mut encoded = vec![0x01];
        encoded.extend_from_slice(&leaf);
        encoded.push(0x00); // right: absent
        encoded.push(0x00); // parents: empty

        let tree = parse_commitment_tree(&hex::encode(&encoded)).unwrap();
        assert_eq!(tree.size(), 1);
        assert!(!tree.is_empty());
    }

    #[test]
    fn a_truncated_tree_state_is_an_error() {
        assert!(matches!(
            parse_commitment_tree("0111"),
            Err(Error::MalformedTree(_))
        ));
    }

    #[test]
    fn a_non_hex_tree_state_is_an_error() {
        assert!(parse_commitment_tree("zz").is_err());
    }
}
