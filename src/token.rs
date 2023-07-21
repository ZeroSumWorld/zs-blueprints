use scrypto::prelude::*;
use std::fmt;


#[blueprint]
mod token {
    use scrypto::borrow_resource_manager;
    use scrypto::model::{ComponentAddress, ResourceAddress};
    use scrypto::prelude::{ComponentAuthZone, Vault};
    struct Token {
        token_address: ResourceAddress,
        minter_badge: Vault
    }
    impl Token {
        pub fn create() -> ComponentAddress {
            let minter_badge: Bucket = ResourceBuilder::new_fungible()
                .divisibility(DIVISIBILITY_NONE)
                .metadata("name", "DxToken MinterBadge")
                .mint_initial_supply(1);
            let token_address: ResourceAddress = ResourceBuilder::new_fungible()
                .divisibility(3)
                .metadata("name", "Dx Token")
                .metadata("symbol", "DXT")
                .mintable(rule!(require(minter_badge.resource_address())), AccessRule::DenyAll)
                .create_with_no_initial_supply();
            info!("DX Token created");
            info!("DX Token Manager Instance created");
            Self {
                token_address,
                minter_badge: Vault::with_bucket(minter_badge)
            }
                .instantiate()
                .globalize()
        }
        pub fn airdrop(&self) -> Bucket {
            ComponentAuthZone::push(self.minter_badge.create_proof());
            info!("Airdrop 1 DXT");
            borrow_resource_manager!(self.token_address).mint(Decimal::from(1))
        }
    }
}