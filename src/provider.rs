use anyhow::bail;
use revm::{
    db::{CacheDB, DatabaseRef, DbAccount, EmptyDB},
    primitives::{
        AccountInfo, Address, ExecutionResult, Log, Output, ResultAndState, TransactTo, TxEnv, U256,
    },
    EVM,
};

use std::sync::{Arc, RwLock};

/// Provider for Revm
#[derive(Clone)]
pub struct RevmProvider {
    // use an inner approach so the provider does not need to be mutable
    inner: Arc<RwLock<EthVmInner>>,
}

// @todo need option to load fork from chain
impl RevmProvider {
    pub fn new() -> Self {
        Self {
            inner: Arc::new(RwLock::new(EthVmInner::new())),
        }
    }

    /// Deploy a contract. Return the contract's address and the amount of gas used
    pub fn deploy(&self, tx: TxEnv) -> anyhow::Result<(Address, u64)> {
        let (output, gas, _) = self
            .inner
            .write()
            .unwrap()
            .write(tx)
            .and_then(|r| process_execution_result(r))?;

        match output {
            Output::Create(_, Some(address)) => Ok((address.into(), gas)),
            _ => bail!("expected a create call"),
        }
    }

    /// Transfer value
    pub fn transfer<T: Into<Address>>(
        &self,
        from: T,
        to: T,
        value: U256,
    ) -> anyhow::Result<(ethers::types::Bytes, u64, Vec<Log>)> {
        let mut tx = TxEnv::default();
        tx.caller = from.into();
        tx.transact_to = TransactTo::Call(to.into());
        tx.value = value;

        self.send(tx)
    }

    /// Send a transaction. Committing to the Evm db
    pub fn send(&self, tx: TxEnv) -> anyhow::Result<(ethers::types::Bytes, u64, Vec<Log>)> {
        self.inner
            .write()
            .unwrap()
            .write(tx)
            .and_then(|r| process_result_with_value(r))
    }

    /// Call a contract (view, pure)
    pub fn call(&self, tx: TxEnv) -> anyhow::Result<(ethers::types::Bytes, u64, Vec<Log>)> {
        self.inner
            .write()
            .unwrap()
            .read(tx)
            .and_then(|r| process_result_with_value(r))
    }

    /// Get the balance for the given user
    pub fn balance_of(&self, user: Address) -> U256 {
        self.inner.write().unwrap().balance_of(user)
    }

    /// Create and account with optional funding
    pub fn create_account(&self, user: Address, value: Option<U256>) -> anyhow::Result<()> {
        self.inner.write().unwrap().add_account(user, value)
    }

    /// View raw details of an account
    pub fn view_account(&self, user: Address) -> anyhow::Result<DbAccount> {
        self.inner.write().unwrap().view_account(user)
    }
}

// Inner wrapper talking to Revm
struct EthVmInner {
    evm: EVM<CacheDB<EmptyDB>>,
}

impl EthVmInner {
    fn new() -> Self {
        let mut evm = EVM::new();
        let db = CacheDB::new(EmptyDB {});
        evm.env.block.gas_limit = U256::MAX;

        // @todo make configurable to include base fee,etc...
        // evm.env.block.basefee = parse_ether(0.000001).unwrap().into();

        evm.database(db);
        Self { evm }
    }

    /// write transaction to the db
    fn write(&mut self, tx: TxEnv) -> anyhow::Result<ExecutionResult> {
        self.evm.env.tx = tx;
        match self.evm.transact_commit() {
            Ok(r) => Ok(r),
            Err(e) => bail!(format!("error with write: {:?}", e)),
        }
    }

    /// read only
    fn read(&mut self, tx: TxEnv) -> anyhow::Result<ExecutionResult> {
        self.evm.env.tx = tx;
        match self.evm.transact_ref() {
            Ok(ResultAndState { result, .. }) => Ok(result),
            _ => bail!("error with simulate write..."),
        }
    }

    fn balance_of(&mut self, user: Address) -> U256 {
        let db = self.evm.db().expect("evm db");
        match db.basic(user) {
            Ok(Some(account)) => account.balance,
            _ => U256::ZERO,
        }
    }

    fn add_account(&mut self, user: Address, value: Option<U256>) -> anyhow::Result<()> {
        let mut info = AccountInfo::default();
        if value.is_some() {
            info.balance = value.unwrap();
        }

        self.evm
            .db()
            .and_then(|db| Some(db.insert_account_info(user, info)));

        Ok(())
    }

    fn view_account(&mut self, user: Address) -> anyhow::Result<DbAccount> {
        match self.evm.db().unwrap().load_account(user) {
            Ok(account) => Ok(account.clone()),
            _ => bail!("ooops"),
        }
    }
}

fn process_execution_result(result: ExecutionResult) -> anyhow::Result<(Output, u64, Vec<Log>)> {
    match result {
        ExecutionResult::Success {
            output,
            gas_used,
            logs,
            ..
        } => Ok((output, gas_used, logs)),
        ExecutionResult::Revert { output, .. } => bail!("Failed due to revert: {:?}", output),
        ExecutionResult::Halt { reason, .. } => bail!("Failed due to halt: {:?}", reason),
    }
}

fn process_result_with_value(
    result: ExecutionResult,
) -> anyhow::Result<(ethers::types::Bytes, u64, Vec<Log>)> {
    let (output, gas_used, logs) = process_execution_result(result)?;
    let bits = match output {
        Output::Call(value) => value,
        _ => bail!("expected call output"),
    };

    Ok((bits.into(), gas_used, logs))
}

#[cfg(test)]
mod test {
    use ethers::utils::parse_ether;

    use crate::prelude::*;

    #[test]
    fn provider_basics() {
        let provider = RevmProvider::new();

        // @todo make ether conversions a little smoother
        let one_ether: U256 = parse_ether(1).unwrap().into();

        let alice = Address::from_low_u64_be(1);
        let bob = Address::from_low_u64_be(2);

        let a1 = provider.create_account(alice, Some(one_ether));
        let b1 = provider.create_account(bob, Some(one_ether));

        assert!(a1.is_ok());
        assert!(b1.is_ok());

        assert_eq!(provider.balance_of(bob), one_ether);
        assert_eq!(provider.balance_of(alice), one_ether);
    }
}
