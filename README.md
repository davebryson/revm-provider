
# RevmProvider 
A provider and contract API for [Revm](https://github.com/bluealloy/revm)

Use contracts directly with Revm - no json-rpc overhead

**This is a work in progress**

## Example
```rust 
use revm_provider::prelude::*;

fn main() {
    let provider = RevmProvider::new();

    // create and fund accounts...
    let bob = Address::from_low_u64_be(2);
    let alice = Address::from_low_u64_be(3);

    let funding = U256::from(1e18); // 1 eth
    provider.create_account(bob, Some(funding)).unwrap();
    provider.create_account(alice, Some(funding)).unwrap();

    // load full metadata
    let meta = ContractMetadata::from("./contracts/Counter.json");

    // deploy (bob)
    let (address, _gas) = Contract::deploy(&provider, bob.into(), meta.bytecode()).unwrap();
    println!("Contact address: {}", address);

    // load instance at the deployed address
    let contract = Contract::from(&meta).at(address);

    // read call to 'number' method
    let (val1, _gas, _logs) = contract
        .call::<_, u32>(&provider, "number", (), alice.into())
        .unwrap();

    println!("current value: {:?}", val1);

    // update the number to 5
    let (_, gas, _) = contract
        .send::<_, ()>(&provider, "setNumber", (5_u32,), alice.into(), None)
        .unwrap();

    println!("setNumber gas cost: {:}", gas);

    // read number again
    let (val2, _, _) = contract
        .call::<_, u32>(&provider, "number", (), alice.into())
        .unwrap();

    println!("updated value: {:?}", val2);

    // view account state for alice
    let dbac = provider.view_account(alice).unwrap();
    println!("~ Alice's account details ~");
    println!("{:?}", dbac);
}

```