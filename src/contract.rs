use anyhow::{anyhow, bail};
use ethers::{
    abi::{Abi, Detokenize, JsonAbi, Tokenize},
    prelude::BaseContract,
};

use revm::primitives::{TransactTo, TxEnv};
use std::{env, fs, path::PathBuf};

use crate::prelude::*;

/// Expects a path relative to the root cargo directory
/// reused from rust-web3
fn normalize_path(relative_path: &str) -> anyhow::Result<PathBuf> {
    // workaround for https://github.com/rust-lang/rust/issues/43860
    let cargo_toml_directory = env::var("CARGO_MANIFEST_DIR").unwrap();
    let mut path: PathBuf = cargo_toml_directory.into();
    path.push(relative_path);
    Ok(path)
}

/// Container for contract metadata
#[derive(Clone)]
pub struct ContractMetadata {
    abi: String,
    bytecode: Bytes,
}

impl ContractMetadata {
    pub fn bytecode(&self) -> Bytes {
        self.bytecode.clone()
    }

    pub fn abi(&self) -> String {
        self.abi.clone()
    }
}

/// Load from file
impl From<&str> for ContractMetadata {
    fn from(path: &str) -> Self {
        let normalized_path = normalize_path(&path).unwrap();
        let source_file = fs::File::open(&normalized_path).unwrap();
        match serde_json::from_reader::<_, JsonAbi>(source_file) {
            Ok(JsonAbi::Object(o)) => {
                let abi = serde_json::to_string(&o.abi).unwrap();
                if o.bytecode.is_none() {
                    panic!("missing bytecode")
                }
                return Self {
                    abi,
                    bytecode: o.bytecode.unwrap().to_vec().into(),
                };
            }
            _ => panic!("expected a full contract metadata json file"),
        }
    }
}

#[derive(Clone)]
pub struct Contract {
    contract: BaseContract,
    pub address: Option<Address>,
}

/// Create a contract for the given ABI.  Adopted from `ethers-rs`.
/// Example:
/// ```
/// use revm_provider::prelude::*;
///
/// let abi = parse_abi(&[
///   "function x() external view returns (uint256)",
/// ]).unwrap();
/// let contract = Contract::from(abi);
/// ```
impl From<Abi> for Contract {
    fn from(abi: Abi) -> Self {
        Self {
            contract: BaseContract::from(abi),
            address: None,
        }
    }
}

/// Create a contract from metadata
impl From<&ContractMetadata> for Contract {
    fn from(abi: &ContractMetadata) -> Self {
        let abi = serde_json::from_str::<Abi>(&abi.abi).unwrap();
        Self {
            contract: BaseContract::from(abi),
            address: None,
        }
    }
}

impl Contract {
    /// Set the deployed address of the contract
    ///
    /// adapted from ethers-rs
    pub fn at<T: Into<Address>>(&self, address: T) -> Self {
        let mut this = self.clone();
        this.address = Some(address.into());
        this
    }

    // @todo add optional value. AND it take constructor args
    pub fn deploy(
        evm: &RevmProvider,
        deployer: Address,
        bincode: Bytes,
    ) -> anyhow::Result<(Address, u64)> {
        let mut tx = TxEnv::default();
        tx.caller = deployer.into();
        tx.transact_to = TransactTo::create();
        tx.data = bincode.to_vec().into();
        //tx.value = U256::zero().into();

        evm.deploy(tx)
    }

    /// Make a read-only request to the contract
    pub fn call<T, D>(
        &self,
        evm: &RevmProvider,
        name: &str,
        args: T,
        caller: Address,
    ) -> anyhow::Result<(D, u64, Vec<Log>)>
    where
        T: Tokenize,
        D: Detokenize,
    {
        if self.address.is_none() {
            bail!("missing contract address");
        }

        let encoded = self.contract.encode(name, args)?;

        let mut tx = TxEnv::default();
        tx.caller = caller.into();
        tx.transact_to = TransactTo::Call(self.address.unwrap().into());
        tx.data = encoded.to_vec().into(); //revm::precompile::Bytes::from(encoded.to_vec());

        evm.call(tx)
            .and_then(|(bits, gas_used, logs)| {
                let v = self.contract.decode_output::<D, _>(name, bits)?;
                Ok((v, gas_used, logs))
            })
            .map_err(|e| anyhow!("{:}", e))
    }

    /// Send a transaction
    pub fn send<T, D>(
        &self,
        evm: &RevmProvider,
        name: &str,
        args: T,
        caller: Address,
        value: Option<U256>,
    ) -> anyhow::Result<(D, u64, Vec<Log>)>
    where
        T: Tokenize,
        D: Detokenize,
    {
        if self.address.is_none() {
            bail!("missing contract address");
        }

        let encoded = self.contract.encode(name, args)?;

        // @todo estimate gas cost for tx
        let mut tx = TxEnv::default();
        tx.caller = caller.into();
        tx.transact_to = TransactTo::Call(self.address.unwrap().into());
        tx.data = encoded.to_vec().into(); //revm::precompile::Bytes::from(encoded.to_vec());

        if value.is_some() {
            tx.value = value.unwrap().into();
        }

        evm.send(tx)
            .and_then(|(bits, gas_used, logs)| {
                let v = self.contract.decode_output::<D, _>(name, bits)?;
                Ok((v, gas_used, logs))
            })
            .map_err(|e| anyhow!("oops {:}", e))
    }
}

#[cfg(test)]
mod tests {

    use crate::prelude::*;

    #[test]
    fn load_contract_metadata() {
        let meta = ContractMetadata::from("./contracts/Counter.json");
        assert!(meta.bytecode().len() > 0);
        assert!(meta.abi().len() > 0);
    }

    #[test]
    #[should_panic]
    fn metadata_panics_on_missing_file() {
        let _ = ContractMetadata::from("./nope.json");
    }
}
