use age::{Recipient, secrecy::ExposeSecret};
use bip0039::{Count, English, Mnemonic};
use rust_decimal::prelude::ToPrimitive;
use secrecy::{ExposeSecret as _, SecretString, SecretVec, Zeroize};
use std::path::{Path, PathBuf};
use tokio::io::AsyncWriteExt;
use tonic::transport::Channel;
use zcash_client_sqlite::WalletDb;

use std::os::raw::c_uchar;
use std::str;
use std::{os, slice};

use libc::c_char;
use std::ffi::{CStr, CString};
use zcash_client_backend::{
    data_api::{Account, AccountBirthday, WalletRead, WalletWrite},
    proto::service::{self, compact_tx_streamer_client::CompactTxStreamerClient},
};
use zcash_protocol::consensus::{self, BlockHeight, Parameters};

use crate::data::get_db_paths;
// use crate::{config::WalletConfig, remote::Servers};

mod config;
mod data;
mod error;
mod remote;

pub async fn create_wallet(wallet_name: String) -> Result<(), anyhow::Error> {
    let wallet_dir = Some(wallet_name.to_owned());
    let network = consensus::Network::MainNetwork;
    let params = consensus::Network::from(network);

    let server = remote::Servers::parse("zecrocks")?; //Servers::pick(&self, network) //Servers.pick(params)?;
    let s2 = server.pick(params)?;
    let mut client = s2.connect_direct().await?;

    let chain_tip: u32 = client
        .get_latest_block(service::ChainSpec::default())
        .await?
        .into_inner()
        .height
        .try_into()
        .expect("block heights must fit into u32");

    println!(" Blocknumber {:?}", chain_tip.to_u32());

    let mut path = PathBuf::from(wallet_dir.to_owned().unwrap());
    path.push("wallet");
    let identity_file_name = path.into_os_string().into_string().unwrap();

    let recipients: Vec<Box<dyn Recipient + Send>> =
        if tokio::fs::try_exists(&identity_file_name).await? {
            age::IdentityFile::from_file(identity_file_name)?.to_recipients()?
        } else {
            /// Generate identity
            let identity = age::x25519::Identity::generate();
            let recipient = identity.to_public();
            let path = PathBuf::from(wallet_dir.to_owned().unwrap());
            // let identity_file_name = String::from("./new_wallet");
            tokio::fs::create_dir_all(path).await?;

            let mut f = tokio::fs::File::create_new(identity_file_name).await?;
            f.write_all(
                format!(
                    "# created: {}\n",
                    chrono::Local::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true)
                )
                .as_bytes(),
            )
            .await?;
            f.write_all(format!("# public key: {recipient}\n").as_bytes())
                .await?;
            f.write_all(format!("{}\n", identity.to_string().expose_secret()).as_bytes())
                .await?;
            f.flush().await?;

            vec![Box::new(recipient) as _]
        };

    let mnemonic = <Mnemonic<English>>::generate(Count::Words24);

    let birthday = get_wallet_birthday(
        client,
        None.unwrap_or(chain_tip.saturating_sub(100)).into(),
        None,
    )
    .await?;

    config::WalletConfig::init_with_mnemonic(
        wallet_dir.as_ref(),
        recipients.iter().map(|r| r.as_ref() as _),
        &mnemonic,
        birthday.height(),
        network.into(),
    )?;

    let seed = {
        let mut seed = mnemonic.to_seed("");
        let secret = seed.to_vec();
        seed.zeroize();
        SecretVec::new(secret)
    };

    let wallet_name = "wallet";

    init_dbs(
        params,
        wallet_dir.as_ref(),
        wallet_name,
        &seed,
        birthday,
        None,
    )
}

pub async fn list_accounts(wallet_name: String) -> Result<(), anyhow::Error> {
    let wallet_dir: Option<String> = Some(wallet_name.to_owned());
    let params = consensus::Network::MainNetwork;
    let (_, db_data) = get_db_paths(wallet_dir.as_ref());
    let db_data = WalletDb::for_path(db_data, params, (), ())?;

    for account_id in db_data.get_account_ids()?.iter() {
        let account = db_data.get_account(*account_id)?.unwrap();
        println!("Account {}", account_id.expose_uuid());
        if let Some(name) = account.name() {
            println!("Name: {name}");
        }
    }

    Ok(())
}

async fn get_wallet_birthday(
    mut client: CompactTxStreamerClient<Channel>,
    birthday_height: BlockHeight,
    recover_until: Option<BlockHeight>,
) -> Result<AccountBirthday, anyhow::Error> {
    // Fetch the tree state corresponding to the last block prior to the wallet's
    // birthday height. NOTE: THIS APPROACH LEAKS THE BIRTHDAY TO THE SERVER!
    let request = service::BlockId {
        height: u64::from(birthday_height).saturating_sub(1),
        ..Default::default()
    };
    let treestate = client.get_tree_state(request).await?.into_inner();
    let birthday =
        AccountBirthday::from_treestate(treestate, recover_until).map_err(error::Error::from)?;

    Ok(birthday)
}

fn init_dbs(
    params: impl Parameters + 'static,
    wallet_dir: Option<&String>,
    account_name: &str,
    seed: &SecretVec<u8>,
    birthday: AccountBirthday,
    key_source: Option<&str>,
) -> Result<(), anyhow::Error> {
    // Initialise the block and wallet DBs.
    let mut db_data = data::init_dbs(params, wallet_dir)?;

    // Add account.
    db_data.create_account(account_name, seed, &birthday, key_source)?;

    Ok(())
}

#[unsafe(no_mangle)]
pub extern "C" fn go_create_wallet(ptr: *const std::os::raw::c_char) {
    let rt = tokio::runtime::Runtime::new().expect("Failed to create Tokio runtime");

    unsafe {
        let c_str = std::ffi::CStr::from_ptr(ptr);
        let r_str = c_str.to_str().expect("Invalid Utf-8");

        let result = rt.block_on(create_wallet(r_str.to_string()));

        if result.is_err() {
            println!("Failed to create wallet")
        }
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn go_list_accounts(ptr: *const std::os::raw::c_char) {
      let rt = tokio::runtime::Runtime::new().expect("Failed to create Tokio runtime");

    unsafe {
        let c_str = std::ffi::CStr::from_ptr(ptr);
        let r_str = c_str.to_str().expect("Invalid Utf-8");

        let result = rt.block_on(list_accounts(r_str.to_string()));

        if result.is_err() {
            println!("Failed to list account of wallet")
        }
    }
}

/// return string
#[unsafe(no_mangle)]
pub unsafe extern "C" fn get_string() -> *mut c_char {
    let rust_string = String::from("Hello from Rust!");
    // Convert Rust String to C string
    let c_string = match CString::new(rust_string) {
        Ok(s) => s,
        Err(_) => return std::ptr::null_mut(),
    };

    // Transfer ownership to caller
    c_string.into_raw()
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn free_string(s: *mut c_char) {
    if s.is_null() {
        return;
    }
    // Reclaim ownership and drop
    unsafe { CString::from_raw(s) };
}
