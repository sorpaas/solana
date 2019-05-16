//! Stakes serve as a cache of stake and vote accounts to derive
//! node stakes
use hashbrown::HashMap;
use solana_sdk::account::Account;
use solana_sdk::pubkey::Pubkey;
use solana_stake_api::stake_state::StakeState;

#[derive(Default, Clone)]
pub struct Stakes {
    /// vote accounts
    vote_accounts: HashMap<Pubkey, (u64, Account)>,

    /// stake_accounts
    stake_accounts: HashMap<Pubkey, Account>,
}

impl Stakes {
    // sum the stakes that point to the given voter_id
    fn calculate_stake(&self, voter_id: &Pubkey) -> u64 {
        self.stake_accounts
            .iter()
            .filter(|(_, stake_account)| {
                Some(*voter_id) == StakeState::voter_id_from(stake_account)
            })
            .map(|(_, stake_account)| stake_account.lamports)
            .sum()
    }

    pub fn is_stake(account: &Account) -> bool {
        solana_vote_api::check_id(&account.owner) || solana_stake_api::check_id(&account.owner)
    }

    pub fn store(&mut self, pubkey: &Pubkey, account: &Account) {
        if solana_vote_api::check_id(&account.owner) {
            if account.lamports == 0 {
                self.vote_accounts.remove(pubkey);
            } else {
                // update the stake of this entry
                let stake = self
                    .vote_accounts
                    .get(pubkey)
                    .map_or_else(|| self.calculate_stake(pubkey), |v| v.0);

                self.vote_accounts.insert(*pubkey, (stake, account.clone()));
            }
        } else if solana_stake_api::check_id(&account.owner) {
            //  old_stake is stake lamports and voter_id from the pre-store() version
            let old_stake = self.stake_accounts.get(pubkey).and_then(|old_account| {
                StakeState::voter_id_from(old_account)
                    .map(|old_voter_id| (old_account.lamports, old_voter_id))
            });

            let stake =
                StakeState::voter_id_from(account).map(|voter_id| (account.lamports, voter_id));

            // if adjustments need to be made...
            if stake != old_stake {
                if let Some((old_stake, old_voter_id)) = old_stake {
                    self.vote_accounts
                        .entry(old_voter_id)
                        .and_modify(|e| e.0 -= old_stake);
                }
                if let Some((stake, voter_id)) = stake {
                    self.vote_accounts
                        .entry(voter_id)
                        .and_modify(|e| e.0 += stake);
                }
            }

            if account.lamports == 0 {
                self.stake_accounts.remove(pubkey);
            } else {
                self.stake_accounts.insert(*pubkey, account.clone());
            }
        }
    }
    pub fn vote_accounts(&self) -> &HashMap<Pubkey, (u64, Account)> {
        &self.vote_accounts
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use solana_sdk::pubkey::Pubkey;
    use solana_stake_api::stake_state;
    use solana_vote_api::vote_state::{self, VoteState};

    //  set up some dummies  for a staked node    ((     vote      )  (     stake     ))
    fn create_staked_node_accounts(stake: u64) -> ((Pubkey, Account), (Pubkey, Account)) {
        let vote_id = Pubkey::new_rand();
        let vote_account = vote_state::create_account(&vote_id, &Pubkey::new_rand(), 0, 1);
        (
            (vote_id, vote_account),
            create_stake_account(stake, &vote_id),
        )
    }

    //   add stake to a vote_id                               (   stake    )
    fn create_stake_account(stake: u64, vote_id: &Pubkey) -> (Pubkey, Account) {
        (
            Pubkey::new_rand(),
            stake_state::create_delegate_stake_account(&vote_id, &VoteState::default(), stake),
        )
    }

    #[test]
    fn test_stakes_basic() {
        let mut stakes = Stakes::default();

        let ((vote_id, vote_account), (stake_id, mut stake_account)) =
            create_staked_node_accounts(10);

        stakes.store(&vote_id, &vote_account);
        stakes.store(&stake_id, &stake_account);

        {
            let vote_accounts = stakes.vote_accounts();
            assert!(vote_accounts.get(&vote_id).is_some());
            assert_eq!(vote_accounts.get(&vote_id).unwrap().0, 10);
        }

        stake_account.lamports = 42;
        stakes.store(&stake_id, &stake_account);
        {
            let vote_accounts = stakes.vote_accounts();
            assert!(vote_accounts.get(&vote_id).is_some());
            assert_eq!(vote_accounts.get(&vote_id).unwrap().0, 42);
        }

        stake_account.lamports = 0;
        stakes.store(&stake_id, &stake_account);
        {
            let vote_accounts = stakes.vote_accounts();
            assert!(vote_accounts.get(&vote_id).is_some());
            assert_eq!(vote_accounts.get(&vote_id).unwrap().0, 0);
        }
    }

    #[test]
    fn test_stakes_vote_account_disappear_reappear() {
        let mut stakes = Stakes::default();

        let ((vote_id, mut vote_account), (stake_id, stake_account)) =
            create_staked_node_accounts(10);

        stakes.store(&vote_id, &vote_account);
        stakes.store(&stake_id, &stake_account);

        {
            let vote_accounts = stakes.vote_accounts();
            assert!(vote_accounts.get(&vote_id).is_some());
            assert_eq!(vote_accounts.get(&vote_id).unwrap().0, 10);
        }

        vote_account.lamports = 0;
        stakes.store(&vote_id, &vote_account);

        {
            let vote_accounts = stakes.vote_accounts();
            assert!(vote_accounts.get(&vote_id).is_none());
        }
        vote_account.lamports = 1;
        stakes.store(&vote_id, &vote_account);

        {
            let vote_accounts = stakes.vote_accounts();
            assert!(vote_accounts.get(&vote_id).is_some());
            assert_eq!(vote_accounts.get(&vote_id).unwrap().0, 10);
        }
    }

    #[test]
    fn test_stakes_change_delegate() {
        let mut stakes = Stakes::default();

        let ((vote_id, vote_account), (stake_id, stake_account)) = create_staked_node_accounts(10);

        let ((vote_id2, vote_account2), (_stake_id2, stake_account2)) =
            create_staked_node_accounts(10);

        stakes.store(&vote_id, &vote_account);
        stakes.store(&vote_id2, &vote_account2);

        // delegates to vote_id
        stakes.store(&stake_id, &stake_account);

        {
            let vote_accounts = stakes.vote_accounts();
            assert!(vote_accounts.get(&vote_id).is_some());
            assert_eq!(vote_accounts.get(&vote_id).unwrap().0, 10);
            assert!(vote_accounts.get(&vote_id2).is_some());
            assert_eq!(vote_accounts.get(&vote_id2).unwrap().0, 0);
        }

        // delegates to vote_id2
        stakes.store(&stake_id, &stake_account2);

        {
            let vote_accounts = stakes.vote_accounts();
            assert!(vote_accounts.get(&vote_id).is_some());
            assert_eq!(vote_accounts.get(&vote_id).unwrap().0, 0);
            assert!(vote_accounts.get(&vote_id2).is_some());
            assert_eq!(vote_accounts.get(&vote_id2).unwrap().0, 10);
        }
    }
    #[test]
    fn test_stakes_multiple_stakers() {
        let mut stakes = Stakes::default();

        let ((vote_id, vote_account), (stake_id, stake_account)) = create_staked_node_accounts(10);

        let (stake_id2, stake_account2) = create_stake_account(10, &vote_id);

        stakes.store(&vote_id, &vote_account);

        // delegates to vote_id
        stakes.store(&stake_id, &stake_account);
        stakes.store(&stake_id2, &stake_account2);

        {
            let vote_accounts = stakes.vote_accounts();
            assert!(vote_accounts.get(&vote_id).is_some());
            assert_eq!(vote_accounts.get(&vote_id).unwrap().0, 20);
        }
    }

    #[test]
    fn test_stakes_not_delegate() {
        let mut stakes = Stakes::default();

        let ((vote_id, vote_account), (stake_id, stake_account)) = create_staked_node_accounts(10);

        stakes.store(&vote_id, &vote_account);
        stakes.store(&stake_id, &stake_account);

        {
            let vote_accounts = stakes.vote_accounts();
            assert!(vote_accounts.get(&vote_id).is_some());
            assert_eq!(vote_accounts.get(&vote_id).unwrap().0, 10);
        }

        // not a stake account, and whacks above entry
        stakes.store(&stake_id, &Account::new(1, 0, &solana_stake_api::id()));
        {
            let vote_accounts = stakes.vote_accounts();
            assert!(vote_accounts.get(&vote_id).is_some());
            assert_eq!(vote_accounts.get(&vote_id).unwrap().0, 0);
        }
    }

}
