use scrypto::prelude::*;
use std::fmt;
use scrypto::prelude::String;


external_component! {
  Account {
    fn deposit(&mut self, arg0: Bucket) -> ();
    fn withdraw_by_amount(&mut self, arg0: Decimal, arg1: ResourceAddress) -> Bucket;
  }
}

#[derive(NonFungibleData)]
struct MatchData {
    #[mutable]
    fee: Decimal,
    #[mutable]
    creator_id: u128,
    #[mutable]
    creator_address: Option<ComponentAddress>,
    #[mutable]
    acceptor_id: u128,
    #[mutable]
    acceptor_address: Option<ComponentAddress>,
    #[mutable]
    winner_id: u128
}

#[blueprint]
mod match_provider {
    use scrypto::{borrow_resource_manager, info};
    use scrypto::math::Decimal;
    use scrypto::model::{Bucket, ComponentAddress, NonFungibleLocalId, ResourceAddress, UUIDNonFungibleLocalId};
    use scrypto::model::NonFungibleIdType::UUID;
    use scrypto::prelude::{ComponentAuthZone, ResourceManager, Vault};
    struct MatchProvider {
        token_vault: Vault,
        auth_badge: Vault,
        matches: Vault,
    }
    impl MatchProvider {
        pub fn new(token_address: ResourceAddress) -> (ComponentAddress, Bucket) {
            let badge: Bucket = ResourceBuilder::new_fungible()
                .divisibility(DIVISIBILITY_NONE)
                .metadata("name", "DX Match Modifier Badge")
                .mint_initial_supply(1);
            let rule = rule!(require(badge.resource_address()));
            let admin_badge: Bucket = ResourceBuilder::new_fungible()
                .divisibility(DIVISIBILITY_NONE)
                .metadata("name", "DX Match Admin Badge")
                .mint_initial_supply(1);
            let matches: ResourceAddress = ResourceBuilder::new_uuid_non_fungible()
                .metadata("name", "DX Match")
                .metadata("description", "DisruptionX matches registrations")
                .mintable(rule.clone(), AccessRule::DenyAll)
                .burnable(rule.clone(), AccessRule::DenyAll)
                .updateable_non_fungible_data(rule, LOCKED)
                .create_with_no_initial_supply();
            info!("MatchProvider Instance created");
            let mut provider_component = Self {
                token_vault: Vault::new(token_address),
                auth_badge: Vault::with_bucket(badge),
                matches: Vault::new(matches)
            }.instantiate();
            let admin_rule = rule!(require(admin_badge.resource_address()));
            let access_rules = AccessRules::new()
                .method("set_winner", admin_rule.clone(), AccessRule::DenyAll)
                .method("cancel_match", admin_rule.clone(), AccessRule::DenyAll)
                .method("cancel_registration", admin_rule, AccessRule::DenyAll)
                .default(AccessRule::AllowAll, AccessRule::DenyAll);
            provider_component.add_access_check(access_rules);
            (provider_component.globalize(),
            admin_badge)
        }
        fn auth(&mut self) {
            ComponentAuthZone::push(self.auth_badge.create_proof());
        }
        fn load_match(&mut self, match_id: u128) -> (MatchData, NonFungibleLocalId) {
            let id = NonFungibleLocalId::UUID(UUIDNonFungibleLocalId::new(match_id).unwrap());
            let manager = borrow_resource_manager!(self.matches.resource_address());
            assert!(manager.non_fungible_exists(&id), "Match not exists");
            (manager.get_non_fungible_data(&id), id)
        }
        fn set_match(&mut self, id: NonFungibleLocalId, data: MatchData) {
            let manager = borrow_resource_manager!(self.matches.resource_address());
            manager.update_non_fungible_data(&id, data);
            debug!("Match updated: {}", id);
        }
        fn delete_match(&mut self, id: NonFungibleLocalId) {
            self.matches.take_non_fungible(&id).burn();
            debug!("Match deleted: {}", id);
        }
        fn take_token(&mut self, from: ComponentAddress, amount: Decimal) {
            self.token_vault.put(Account::at(from).withdraw_by_amount(amount, self.token_vault.resource_address()));
            debug!("Took token: {}", amount);
        }
        fn send_token(&mut self, to: Option<ComponentAddress>, amount: Decimal) {
            assert_ne!(to.is_none(), true, "Cannot send to zero address");
            Account::at(to.unwrap()).deposit(self.token_vault.take(amount));
            debug!("Gave token: {}", amount);
        }
        pub fn register(&mut self, match_id: u128, user_id: u128, amount: Decimal, address: ComponentAddress) {
            self.auth();
            let id = NonFungibleLocalId::UUID(UUIDNonFungibleLocalId::new(match_id).unwrap());
            let manager = borrow_resource_manager!(self.matches.resource_address());
            self.take_token(address, amount);
            if manager.non_fungible_exists(&id) {
                let mut data: MatchData = manager.get_non_fungible_data(&id);
                assert_eq!(data.acceptor_id, 0, "Lobby is full");
                assert_eq!(data.fee, amount, "Fee amounts do not match");
                assert_ne!(data.creator_id, user_id, "Registration twice not allowed");
                data.acceptor_id = user_id;
                data.acceptor_address = Some(address);
                info!("Match accepted: {}, acceptor: {}", match_id, user_id);
                self.set_match(id, data);
            }
            else {
                let data = MatchData {
                    fee: amount,
                    creator_id: user_id,
                    creator_address: Some(address),
                    acceptor_id: 0,
                    acceptor_address: None,
                    winner_id: 0
                };
                let new_match: Bucket = manager.mint_non_fungible(&id, data);
                self.matches.put(new_match);
                info!("New match created: {}, creator: {}", match_id, user_id);
                info!("Fee amount: {}", amount);
            }
        }
        pub fn set_winner(&mut self, match_id: u128, user_id: u128) {
            self.auth();
            let (data, id) = self.load_match(match_id);
            assert_ne!(data.acceptor_id, 0, "Missing Acceptor");
            assert_eq!(data.winner_id, 0, "Already finished");
            if data.acceptor_id == user_id {
                self.send_token(data.acceptor_address, data.fee * 2);
            }
            else if data.creator_id == user_id {
                self.send_token(data.creator_address, data.fee * 2);
            }
            else {
                assert!(false, "Winner is not a player");
            }
            let mut data = data;
            data.winner_id = user_id;
            info!("Match finished: {}, winner: {}", match_id, user_id);
            self.set_match(id, data);
        }
        pub fn cancel_match(&mut self, match_id: u128) {
            self.auth();
            let (data, id) = self.load_match(match_id);
            assert_eq!(data.winner_id, 0, "Already finished");
            self.send_token(data.creator_address, data.fee);
            if data.acceptor_id != 0 {
                self.send_token(data.acceptor_address, data.fee);
            }
            info!("Match canceled: {}", match_id);
            self.delete_match(id);
        }
        pub fn cancel_registration(&mut self, match_id: u128, user_id: u128) {
            self.auth();
            let (data, id) = self.load_match(match_id);
            assert_eq!(data.winner_id, 0, "Already finished");
            assert_eq!(data.acceptor_id, user_id, "Not an Acceptor");
            self.send_token(data.acceptor_address, data.fee);
            let mut data = data;
            data.acceptor_id = 0;
            data.acceptor_address = None;
            info!("Registration canceled for match: {}, acceptor: {}", match_id, user_id);
            self.set_match(id, data);
        }
    }
}