use criterion::Criterion;
use revm::{
    bytecode::opcode,
    context::TxEnv,
    database::{InMemoryDB, BENCH_CALLER, BENCH_TARGET},
    primitives::{address, Address, TxKind, U256},
    state::{AccountInfo, Bytecode},
    Context, ExecuteCommitEvm, ExecuteEvm, MainBuilder, MainContext,
};
use std::time::{Duration, Instant};

const SUBCALL_TARGET_A: Address = address!("0xaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa");
const SUBCALL_TARGET_B: Address = address!("0xbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb");

fn make_loop_call_bytecode(target: Address) -> Bytecode {
    let mut code = vec![
        opcode::PUSH2,
        0x03,
        0xE8,             // PUSH2 1000 - loop counter
        opcode::JUMPDEST, // loop_start at offset 3
        opcode::PUSH1,
        0x00, // retSize
        opcode::PUSH1,
        0x00, // retOffset
        opcode::PUSH1,
        0x00, // argsSize
        opcode::PUSH1,
        0x00, // argsOffset
        opcode::PUSH1,
        0x00,           // value (no transfer)
        opcode::PUSH20, // target address
    ];
    code.extend_from_slice(target.as_slice());
    code.extend_from_slice(&[
        opcode::GAS, // forward all remaining gas
        opcode::CALL,
        opcode::POP, // discard success/failure
        opcode::PUSH1,
        0x01, // decrement counter
        opcode::SWAP1,
        opcode::SUB,
        opcode::DUP1, // duplicate counter for JUMPI check
        opcode::PUSH1,
        0x03,          // jump target (JUMPDEST offset)
        opcode::JUMPI, // jump back if counter != 0
        opcode::POP,   // clean up remaining counter (0)
        opcode::STOP,
    ]);
    Bytecode::new_raw(code.into())
}

fn make_stop_bytecode() -> Bytecode {
    Bytecode::new_raw([opcode::STOP].into())
}

fn make_subcall_bytecode(target: Address) -> Bytecode {
    let mut code = vec![
        opcode::PUSH1,
        0x00, // retSize
        opcode::PUSH1,
        0x00, // retOffset
        opcode::PUSH1,
        0x00, // argsSize
        opcode::PUSH1,
        0x00, // argsOffset
        opcode::PUSH1,
        0x00,           // value (no transfer)
        opcode::PUSH20, // target address
    ];
    code.extend_from_slice(target.as_slice());
    code.extend_from_slice(&[opcode::GAS, opcode::CALL, opcode::POP, opcode::STOP]);
    Bytecode::new_raw(code.into())
}

fn make_nested_db() -> InMemoryDB {
    let mut db = InMemoryDB::default();
    db.insert_account_info(
        BENCH_CALLER,
        AccountInfo {
            balance: U256::from(u128::MAX),
            ..Default::default()
        },
    );
    db.insert_account_info(
        BENCH_TARGET,
        AccountInfo {
            code: Some(make_loop_call_bytecode(SUBCALL_TARGET_A)),
            ..Default::default()
        },
    );
    db.insert_account_info(
        SUBCALL_TARGET_A,
        AccountInfo {
            code: Some(make_subcall_bytecode(SUBCALL_TARGET_B)),
            ..Default::default()
        },
    );
    db.insert_account_info(
        SUBCALL_TARGET_B,
        AccountInfo {
            code: Some(make_stop_bytecode()),
            ..Default::default()
        },
    );
    db
}

fn make_tx() -> TxEnv {
    TxEnv::builder()
        .caller(BENCH_CALLER)
        .kind(TxKind::Call(BENCH_TARGET))
        .gas_limit(u64::MAX)
        .build()
        .unwrap()
}

pub fn run(criterion: &mut Criterion) {
    let make_evm = || {
        Context::mainnet()
            .with_db(make_nested_db())
            .modify_cfg_chained(|cfg| {
                cfg.disable_nonce_check = true;
                cfg.tx_gas_limit_cap = Some(u64::MAX);
            })
            .build_mainnet()
    };
    let tx = make_tx();

    criterion.bench_function("subcall_1000_nested_transact_one", |b| {
        b.iter_custom(|iters| {
            let mut total = Duration::ZERO;
            for _ in 0..iters {
                let mut evm = make_evm();
                let start = Instant::now();
                evm.transact_one(tx.clone()).unwrap();
                total += start.elapsed();
                drop(evm);
            }
            total
        });
    });

    criterion.bench_function("subcall_1000_nested_transact", |b| {
        b.iter_custom(|iters| {
            let mut total = Duration::ZERO;
            for _ in 0..iters {
                let mut evm = make_evm();
                let start = Instant::now();
                evm.transact(tx.clone()).unwrap();
                total += start.elapsed();
                drop(evm);
            }
            total
        });
    });

    criterion.bench_function("subcall_1000_nested_transact_commit", |b| {
        b.iter_custom(|iters| {
            let mut total = Duration::ZERO;
            for _ in 0..iters {
                let mut evm = make_evm();
                let start = Instant::now();
                evm.transact_commit(tx.clone()).unwrap();
                total += start.elapsed();
                drop(evm);
            }
            total
        });
    });
}
