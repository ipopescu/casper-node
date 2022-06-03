use std::{collections::BTreeSet, env::temp_dir, sync::Arc};

use rand::RngCore;

use casper_types::{testing::TestRng, PublicKey, Timestamp, U512};

use crate::{
    components::consensus::{
        cl_context::{ClContext, Keypair},
        config::Config,
        consensus_protocol::{ConsensusProtocol, ProtocolOutcome},
        highway_core::{
            highway::{SignedWireUnit, Vertex, WireUnit},
            highway_testing,
            state::{self, tests::ALICE, Observation, Panorama},
            validators::ValidatorIndex,
            State,
        },
        protocols::highway::{
            config::Config as HighwayConfig, HighwayMessage, HighwayProtocol, ACTION_ID_VERTEX,
        },
        tests::utils::{
            new_test_chainspec, ALICE_NODE_ID, ALICE_PUBLIC_KEY, ALICE_SECRET_KEY, BOB_PUBLIC_KEY,
        },
        traits::Context,
    },
    types::BlockPayload,
};

/// Returns a new `State` with `ClContext` parameters suitable for tests.
pub(crate) fn new_test_state<I, T>(weights: I, seed: u64) -> State<ClContext>
where
    I: IntoIterator<Item = T>,
    T: Into<state::Weight>,
{
    let params = state::Params::new(
        seed,
        highway_testing::TEST_BLOCK_REWARD,
        highway_testing::TEST_BLOCK_REWARD / 5,
        14,
        19,
        4,
        u64::MAX,
        0.into(),
        Timestamp::MAX,
        highway_testing::TEST_ENDORSEMENT_EVIDENCE_LIMIT,
    );
    let weights = weights.into_iter().map(|w| w.into()).collect::<Vec<_>>();
    state::State::new(weights, params, vec![], vec![])
}

const INSTANCE_ID_DATA: &[u8; 1] = &[123u8; 1];

pub(crate) fn new_test_highway_protocol<I1, I2, T>(
    weights: I1,
    init_faulty: I2,
    rng: &mut TestRng,
) -> Box<dyn ConsensusProtocol<ClContext>>
where
    I1: IntoIterator<Item = (PublicKey, T)>,
    I2: IntoIterator<Item = PublicKey>,
    T: Into<U512>,
{
    let weights = weights
        .into_iter()
        .map(|(pk, w)| (pk, w.into()))
        .collect::<Vec<_>>();
    let chainspec = new_test_chainspec(weights.clone());
    let config = Config {
        max_execution_delay: 3,
        highway: HighwayConfig {
            pending_vertex_timeout: "1min".parse().unwrap(),
            log_participation_interval: Some("10sec".parse().unwrap()),
            ..HighwayConfig::default()
        },
        ..Default::default()
    };

    let wal_file = temp_dir().join(format!("wal{}", rng.next_u64()));

    // Timestamp of the genesis era start and test start.
    let start_timestamp: Timestamp = 0.into();
    let (hw_proto, outcomes) = HighwayProtocol::<ClContext>::new_boxed(
        ClContext::hash(INSTANCE_ID_DATA),
        weights.into_iter().collect(),
        &init_faulty.into_iter().collect(),
        &None.into_iter().collect(),
        &chainspec,
        &config,
        None,
        start_timestamp,
        0,
        start_timestamp,
        wal_file,
    )
    .unwrap();
    // We expect three messages:
    // * log participation timer,
    // * log synchronizer queue length timer,
    // * purge synchronizer queue timer
    // If there are more, the tests might need to handle them.
    assert_eq!(3, outcomes.len());
    hw_proto
}

#[test]
fn test_highway_protocol_handle_message_parse_error() {
    // Build a highway_protocol for instrumentation
    let mut rng = TestRng::new();
    let mut highway_protocol: Box<dyn ConsensusProtocol<ClContext>> =
        new_test_highway_protocol(vec![(ALICE_PUBLIC_KEY.clone(), 100)], vec![], &mut rng);

    let now = Timestamp::zero();
    let sender = *ALICE_NODE_ID;
    let msg = vec![];
    let outcomes = highway_protocol.handle_message(&mut rng, sender.to_owned(), msg, now);
    assert_eq!(&*outcomes, [ProtocolOutcome::Disconnect(sender)]);
}

pub(crate) const N: Observation<ClContext> = Observation::None;

#[test]
fn send_a_wire_unit_with_too_small_a_round_exp() {
    let mut rng = TestRng::new();
    let creator: ValidatorIndex = ValidatorIndex(0);
    let validators = vec![(ALICE_PUBLIC_KEY.clone(), 100)];
    let state: State<ClContext> = new_test_state(validators.iter().map(|(_pk, w)| *w), 0);
    let panorama: Panorama<ClContext> = Panorama::from(vec![N]);
    let seq_number = panorama.next_seq_num(&state, creator);
    let now = Timestamp::zero();
    let wunit: WireUnit<ClContext> = WireUnit {
        panorama,
        creator,
        instance_id: ClContext::hash(INSTANCE_ID_DATA),
        value: None,
        seq_number,
        timestamp: now,
        round_exp: 0,
        endorsed: BTreeSet::new(),
    };
    let alice_keypair: Keypair = Keypair::from(Arc::clone(&*ALICE_SECRET_KEY));
    let highway_message: HighwayMessage<ClContext> = HighwayMessage::NewVertex(Vertex::Unit(
        SignedWireUnit::new(wunit.into_hashed(), &alice_keypair),
    ));
    let mut highway_protocol = new_test_highway_protocol(validators, vec![], &mut rng);
    let sender = *ALICE_NODE_ID;
    let msg = bincode::serialize(&highway_message).unwrap();
    let outcomes = highway_protocol.handle_message(&mut rng, sender.to_owned(), msg, now);
    assert_eq!(&*outcomes, [ProtocolOutcome::Disconnect(sender)]);
}

#[test]
fn send_a_valid_wire_unit() {
    let mut rng = TestRng::new();
    let creator: ValidatorIndex = ValidatorIndex(0);
    let validators = vec![(ALICE_PUBLIC_KEY.clone(), 100)];
    let state: State<ClContext> = new_test_state(validators.iter().map(|(_pk, w)| *w), 0);
    let panorama: Panorama<ClContext> = Panorama::from(vec![N]);
    let seq_number = panorama.next_seq_num(&state, creator);
    let now = Timestamp::zero();
    let wunit: WireUnit<ClContext> = WireUnit {
        panorama,
        creator,
        instance_id: ClContext::hash(INSTANCE_ID_DATA),
        value: Some(Arc::new(BlockPayload::new(vec![], vec![], vec![], false))),
        seq_number,
        timestamp: now,
        round_exp: 14,
        endorsed: BTreeSet::new(),
    };
    let alice_keypair: Keypair = Keypair::from(Arc::clone(&*ALICE_SECRET_KEY));
    let highway_message: HighwayMessage<ClContext> = HighwayMessage::NewVertex(Vertex::Unit(
        SignedWireUnit::new(wunit.into_hashed(), &alice_keypair),
    ));

    let mut highway_protocol = new_test_highway_protocol(validators, vec![], &mut rng);
    let sender = *ALICE_NODE_ID;
    let msg = bincode::serialize(&highway_message).unwrap();

    let mut outcomes = highway_protocol.handle_message(&mut rng, sender, msg, now);
    while let Some(outcome) = outcomes.pop() {
        match outcome {
            ProtocolOutcome::CreatedGossipMessage(_)
            | ProtocolOutcome::FinalizedBlock(_)
            | ProtocolOutcome::HandledProposedBlock(_) => (),
            ProtocolOutcome::QueueAction(ACTION_ID_VERTEX) => {
                outcomes.extend(highway_protocol.handle_action(ACTION_ID_VERTEX, now))
            }
            outcome => panic!("Unexpected outcome: {:?}", outcome),
        }
    }
}

#[test]
fn detect_doppelganger() {
    let mut rng = TestRng::new();
    let creator: ValidatorIndex = ALICE;
    let validators = vec![
        (ALICE_PUBLIC_KEY.clone(), 100),
        (BOB_PUBLIC_KEY.clone(), 100),
    ];
    let state: State<ClContext> = new_test_state(validators.iter().map(|(_pk, w)| *w), 0);
    let panorama: Panorama<ClContext> = Panorama::from(vec![N, N]);
    let seq_number = panorama.next_seq_num(&state, creator);
    let instance_id = ClContext::hash(INSTANCE_ID_DATA);
    let round_exp = 14;
    let now = Timestamp::zero();
    let value = Arc::new(BlockPayload::new(vec![], vec![], vec![], false));
    let wunit: WireUnit<ClContext> = WireUnit {
        panorama,
        creator,
        instance_id,
        value: Some(value),
        seq_number,
        timestamp: now,
        round_exp,
        endorsed: BTreeSet::new(),
    };
    let alice_keypair: Keypair = Keypair::from(Arc::clone(&*ALICE_SECRET_KEY));
    let highway_message: HighwayMessage<ClContext> = HighwayMessage::NewVertex(Vertex::Unit(
        SignedWireUnit::new(wunit.into_hashed(), &alice_keypair),
    ));
    let mut highway_protocol = new_test_highway_protocol(validators, vec![], &mut rng);
    // Activate ALICE as validator.
    let _ = highway_protocol.activate_validator(ALICE_PUBLIC_KEY.clone(), alice_keypair, now, None);
    assert!(highway_protocol.is_active());
    let sender = *ALICE_NODE_ID;
    let msg = bincode::serialize(&highway_message).unwrap();
    // "Send" a message created by ALICE to an instance of Highway where she's an active validator.
    // An incoming unit, created by the same validator, should be properly detected as a
    // doppelganger.
    let mut outcomes = highway_protocol.handle_message(&mut rng, sender, msg, now);
    while let Some(outcome) = outcomes.pop() {
        match outcome {
            ProtocolOutcome::DoppelgangerDetected => return,
            ProtocolOutcome::QueueAction(ACTION_ID_VERTEX) => {
                outcomes.extend(highway_protocol.handle_action(ACTION_ID_VERTEX, now))
            }
            _ => (),
        }
    }
    panic!("failed to return DoppelgangerDetected effect");
}
