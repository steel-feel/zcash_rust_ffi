#![allow(deprecated)]
use std::{num::NonZeroUsize, str::FromStr};

use anyhow::anyhow;
use rand::rngs::OsRng;
use secrecy::ExposeSecret;
use uuid::Uuid;

use zcash_address::ZcashAddress;
use zcash_client_backend::{
     data_api::{
        Account, WalletRead,
        wallet::{
            ConfirmationsPolicy, SpendingKeys, create_proposed_transactions,
            input_selection::GreedyInputSelector, propose_transfer,
        },
    }, fees::{DustOutputPolicy, SplitPolicy, StandardFeeRule, standard::MultiOutputChangeStrategy}, proto::service, wallet::OvkPolicy
};
use zcash_client_sqlite::{WalletDb, util::SystemClock};
use zcash_keys::keys::UnifiedSpendingKey;
use zcash_proofs::prover::LocalTxProver;
use zcash_protocol::{
    ShieldedProtocol,
    memo::{Memo, MemoBytes},
    value::Zatoshis,
};
use zip321::{Payment, TransactionRequest};

use crate::{
    
    config::{WalletConfig, get_identity_file, get_wallet_network, select_account},
    data::get_db_paths,
    error,
    remote::{self},
};

pub async fn send_txn(
    wallet_name: String,
    account_id: Option<Uuid>,
    target_note_count : Option<usize>,
    min_split_output_value: Option<u64>,
    address : String,
    value : u64,
    memo : Option<String>

) -> Result<String, anyhow::Error> {
    let wallet_dir: Option<String> = Some(wallet_name.to_owned());
    let mut config = WalletConfig::read(wallet_dir.as_ref())?;
    let params = get_wallet_network(wallet_dir.as_ref())?;

    let (_, db_data) = get_db_paths(wallet_dir.as_ref());
    let mut db_data = WalletDb::for_path(db_data, params, SystemClock, OsRng)?;
   
    let account = select_account(&db_data, account_id)?;
    let derivation = account
        .source()
        .key_derivation()
        .ok_or(anyhow!("Cannot spend from view-only accounts"))?;

    let identity = get_identity_file(&wallet_name);
    // Decrypt the mnemonic to access the seed.
    let identities = age::IdentityFile::from_file(identity)?.into_identities()?;
    let seed = config
        .decrypt_seed(identities.iter().map(|i| i.as_ref() as _))?
        .ok_or(anyhow!("Seed must be present to enable sending"))?;

    let usk =
        UnifiedSpendingKey::from_seed(&params, seed.expose_secret(), derivation.account_index())
            .map_err(error::Error::from)?;

    let server = remote::Servers::parse("zecrocks")?; //Servers::pick(&self, network) //Servers.pick(params)?;
    let s2 = server.pick(params)?;
    let mut client = s2.connect_direct().await?;

    // Create the transaction.
    println!("Creating transaction...");
    let prover = LocalTxProver::bundled();
    let change_strategy = MultiOutputChangeStrategy::new(
        StandardFeeRule::Zip317,
        None,
        ShieldedProtocol::Orchard,
        DustOutputPolicy::default(),
        SplitPolicy::with_min_output_value(
            NonZeroUsize::new(target_note_count.unwrap_or(4))
                .ok_or(anyhow!("target note count must be nonzero"))?,
            Zatoshis::from_u64(min_split_output_value.unwrap_or(10000000))?,
        ),
    );
    let input_selector = GreedyInputSelector::new();

    let payment = Payment::new(
        ZcashAddress::from_str(&address).map_err(|_| error::Error::InvalidRecipient)?,
        Zatoshis::from_u64(value).map_err(|_| error::Error::InvalidAmount)?,
        memo
            .as_ref()
            .map(|m| Memo::from_str(m))
            .transpose()
            .map_err(|_| error::Error::InvalidMemo)?
            .map(MemoBytes::from),
        None,
        None,
        vec![],
    )
    .expect("payment construction is valid");
    let request = TransactionRequest::new(vec![payment]).map_err(error::Error::from)?;

    let proposal = propose_transfer(
        &mut db_data,
        &params,
        account.id(),
        &input_selector,
        &change_strategy,
        request,
        ConfirmationsPolicy::default(),
    )
    .map_err(error::Error::from)?;

    let txids = create_proposed_transactions(
        &mut db_data,
        &params,
        &prover,
        &prover,
        &SpendingKeys::from_unified_spending_key(usk),
        OvkPolicy::Sender,
        &proposal,
    )
    .map_err(error::Error::from)?;

    if txids.len() > 1 {
        return Err(anyhow!(
            "Multi-transaction proposals are not yet supported."
        ));
    }

    let txid = *txids.first();

    // Send the transaction.
    println!("Sending transaction...");
    let (txid, raw_tx) = db_data
        .get_transaction(txid)?
        .map(|tx| {
            let mut raw_tx = service::RawTransaction::default();
            tx.write(&mut raw_tx.data).unwrap();
            (tx.txid(), raw_tx)
        })
        .ok_or(anyhow!("Transaction not found for id {:?}", txid))?;
      println!("Done Raw transaction...");
    let response = client.send_transaction(raw_tx).await?.into_inner();
      println!("txn sent...{:?}", txid);

    if response.error_code != 0 {
        Err(error::Error::SendFailed {
            code: response.error_code,
            reason: response.error_message,
        }
        .into())
    } else {
        Ok(txid.to_string())
    }
}
