use anyhow::anyhow;
use uuid::Uuid;
use zcash_client_backend::{
    data_api::{Account as _, WalletRead, wallet::ConfirmationsPolicy},
};
use zcash_client_sqlite::WalletDb;

use crate::{
    config::{get_wallet_network, select_account},
    data::get_db_paths,
  
};

pub struct Balance {
    pub height: String,
    pub unshielded: u64,
    pub orchard: u64,
    pub sapling: u64,
    pub total: u64,
}

pub async fn wallet_balance(
    wallet_name: String,
    account_id: Option<Uuid>,
) -> Result<Balance, anyhow::Error> {
    let wallet_dir: Option<String> = Some(wallet_name.to_owned());
    let params = get_wallet_network(wallet_dir.as_ref())?;

    let (_, db_data) = get_db_paths(wallet_dir.as_ref());
    let db_data = WalletDb::for_path(db_data, params, (), ())?;
    let account = select_account(&db_data, account_id)?;

    if let Some(wallet_summary) = db_data.get_wallet_summary(ConfirmationsPolicy::default())? {
        let balance = wallet_summary
            .account_balances()
            .get(&account.id())
            .ok_or_else(|| anyhow!("Missing account 0"))?;

            

        return Ok(Balance {
            height: wallet_summary.chain_tip_height().to_string(),
            unshielded: balance.unshielded_balance().spendable_value().into_u64(),
            orchard: balance.orchard_balance().spendable_value().into_u64(),
            sapling: balance.sapling_balance().spendable_value().into_u64(),
            total: balance.total().into_u64(),
        });
    } else {
        return Err(anyhow!("Unable to fetch balances"));
    }
}
