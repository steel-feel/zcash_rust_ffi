use age::{Recipient, secrecy::ExposeSecret};
use bip0039::{Count, English, Mnemonic};
use rust_decimal::prelude::ToPrimitive;
use secrecy::{SecretVec, Zeroize};
use std::path::PathBuf;
use tokio::io::AsyncWriteExt;
use tonic::{ transport::Channel};
use uuid::Uuid;
use zcash_client_sqlite::WalletDb;
use zcash_keys::keys::UnifiedAddressRequest;

use std::str::{self, FromStr};

use libc::c_char;
use std::ffi::CString;
use zcash_client_backend::{
    data_api::{Account, AccountBirthday, WalletRead, WalletWrite},
    proto::service::{self, compact_tx_streamer_client::CompactTxStreamerClient},
};
use zcash_protocol::consensus::{self, BlockHeight, Parameters};

use crate::config::{get_wallet_network, select_account};
use crate::data::get_db_paths;
use crate::send::send_txn;
use crate::sync::sync;
use crate::{balance::wallet_balance, txn::txn_list};
// use crate::{config::WalletConfig, remote::Servers};

mod balance;
mod config;
mod data;
mod error;
mod remote;
mod send;
mod sync;
mod txn;

pub async fn create_wallet(wallet_name: String, network : bool) -> Result<(), anyhow::Error> {
    let wallet_dir = Some(wallet_name.to_owned());

    let network = if network {
        consensus::Network::MainNetwork
     }else {
        consensus::Network::TestNetwork
     };

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

    let mut path = PathBuf::from(wallet_dir.to_owned().expect("identity path issue"));
    path.push("wallet");
    let identity_file_name = path
        .into_os_string()
        .into_string()
        .expect("Identity file parse issue");

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

pub struct VAccount {
    pub uuid: String,
    pub uivk: String,
    pub ufvk: String,
    pub source: String,
}

pub struct VAccountAddress {
    pub t_address: String,
    pub u_address: String,
}

pub fn get_address(wallet_name: String, uuid: String) -> Result<VAccountAddress, anyhow::Error> {
    let wallet_dir: Option<String> = Some(wallet_name.to_owned());
    let params = get_wallet_network(wallet_dir.as_ref())?;
    let (_, db_data) = get_db_paths(wallet_dir.as_ref());
    let db_data = WalletDb::for_path(db_data, params, (), ())?;
    let account_id = Some(Uuid::from_str(&uuid)?);
    let account = select_account(&db_data, account_id)?;
    let (ua, _) = account
        .uivk()
        .default_address(UnifiedAddressRequest::AllAvailableKeys)?;

    // Note: below also gives Unified address
    // ua.to_zcash_address(params.network_type()).to_string();
    Ok(VAccountAddress {
        u_address: ua.encode(&params),
        t_address: ua
            .transparent()
            .unwrap()
            .to_zcash_address(params.network_type()).to_string(),
    })
}

pub fn list_accounts(wallet_name: String) -> Result<Vec<VAccount>, anyhow::Error> {
    let wallet_dir: Option<String> = Some(wallet_name.to_owned());
    let params = get_wallet_network(wallet_dir.as_ref())?;

    //  zcash_primitives::transaction::Transaction::read(reader, consensus_branch_id)

    let (_, db_data) = get_db_paths(wallet_dir.as_ref());
    let db_data = WalletDb::for_path(db_data, params, (), ())?;

    let mut accounts_list: Vec<VAccount> = vec![];
    for account_id in db_data.get_account_ids()?.iter() {
        let account = db_data.get_account(*account_id)?.unwrap();

        accounts_list.push(VAccount {
            uuid: account_id.expose_uuid().to_string(),
            uivk: account.uivk().encode(&params),
            ufvk: account
                .ufvk()
                .map_or("None".to_owned(), |k| k.encode(&params)),
            source: format!(
                "{:?}",
                account
                    .source()
                    .key_derivation()
                    .unwrap()
                    .seed_fingerprint()
                    .to_string()
            ),
        });
    }

    Ok(accounts_list)
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

//~~~~~~ C bindings ~~~~~~~~~~~

#[repr(C)]
pub struct CAccount {
    uuid: *mut c_char,
    uivk: *mut c_char,
    ufvk: *mut c_char,
    source: *mut c_char,
}

#[repr(C)]
pub struct CAddress {
    t_address: *mut c_char,
    u_address: *mut c_char,
}


#[repr(C)]
pub struct CAccountArray {
    ptr: *mut CAccount,
    len: usize,
}

#[repr(C)]
pub struct CBalance {
    height: *mut c_char,
    total: u64,
    orchard: u64,
    unshielded: u64,
}

#[unsafe(no_mangle)]
pub extern "C" fn go_create_wallet(ptr: *const std::os::raw::c_char, network: u32 ) {
    let rt = tokio::runtime::Runtime::new().expect("Failed to create Tokio runtime");

    unsafe {
        let c_str = std::ffi::CStr::from_ptr(ptr);
        let r_str = c_str.to_str().expect("Invalid Utf-8");

        let c_network = network != 0;

        let result = rt.block_on(create_wallet(r_str.to_string(), c_network));

        if result.is_err() {
            println!("Failed to create wallet")
        }
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn go_list_accounts(ptr: *const std::os::raw::c_char) -> CAccountArray {
    unsafe {
        let c_str = std::ffi::CStr::from_ptr(ptr);
        let r_str = c_str.to_str().expect("Invalid Utf-8");

        let result = list_accounts(r_str.to_string());
        if result.is_err() {
            // println!("Failed to list account of wallet");
            panic!("Failed to list accounts of wallet")
        }

        let mut data: Vec<CAccount> = result
            .unwrap()
            .into_iter()
            .map(|obj| CAccount {
                uuid: CString::new(obj.uuid).unwrap().into_raw(),
                ufvk: CString::new(obj.ufvk).unwrap().into_raw(),
                uivk: CString::new(obj.uivk).unwrap().into_raw(),
                source: CString::new(obj.source).unwrap().into_raw(),
            })
            .collect();

        let len = data.len();
        let ptr = data.as_mut_ptr();
        std::mem::forget(data);

        CAccountArray { ptr, len }
    }
}




#[unsafe(no_mangle)]
pub unsafe extern "C" fn go_get_address(
    ptr: *const std::os::raw::c_char,
    uuid: *const std::os::raw::c_char,
) -> CAddress {
    unsafe {
        let c_ptr_str = std::ffi::CStr::from_ptr(ptr);
        let wallet_dir = c_ptr_str.to_str().expect("Invalid Utf-8");

        let c_uuid_str = std::ffi::CStr::from_ptr(uuid);
        let uuid_str = c_uuid_str.to_str().expect("Invalid Utf-8");

        let acc_address = get_address(wallet_dir.to_owned(), uuid_str.to_owned());
        // Convert Rust String to C string
        // let c_string = 
    
        match acc_address {
            Ok(a) => {
               let t_address =
                match CString::new(a.t_address) {
                    Ok(t) => t.into_raw(),
                    Err(_) =>  { CString::new("").unwrap().into_raw() }
                };

                let u_address =  match CString::new(a.u_address) {
                    Ok(t) => t.into_raw(),
                    Err(_) =>  { CString::new("").unwrap().into_raw() }
                };

                return CAddress{ 
                    t_address,
                    u_address
                };
            } ,
            Err(_) => {}
        };

        // Transfer ownership to caller
        // c_string.into_raw()
        return CAddress{
            t_address : CString::new("").unwrap().into_raw(),
            u_address : CString::new("").unwrap().into_raw()
        };
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn go_sync(ptr: *const std::os::raw::c_char) {
    let rt = tokio::runtime::Runtime::new().expect("Failed to create Tokio runtime");

    unsafe {
        let c_str = std::ffi::CStr::from_ptr(ptr);
        let r_str = c_str.to_str().expect("Invalid Utf-8");

        let result = rt.block_on(sync(r_str.to_string()));

        if result.is_err() {
            println!("Failed to sync wallet")
        }
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn go_balance(
    ptr: *const std::os::raw::c_char,
    uuid: *const std::os::raw::c_char,
) -> CBalance {
    let rt = tokio::runtime::Runtime::new().expect("Failed to create Tokio runtime");
    unsafe {
        let c_str = std::ffi::CStr::from_ptr(ptr);
        let r_str = c_str.to_str().expect("Invalid Utf-8");

        let c_uuid = std::ffi::CStr::from_ptr(uuid);
        let r_uuid = c_uuid.to_str().expect("Invalid Utf-8");

        let account_id = Some(Uuid::from_str(r_uuid).expect("wrong UUid"));

        let result = rt.block_on(wallet_balance(r_str.to_string(), account_id));

        if result.is_err() {
            println!("Failed to check wallet balance");
            return CBalance {
                height: CString::new("").unwrap().into_raw(),
                total: 0,
                orchard: 0,
                unshielded: 0,
            };
        }

        let balance = result.unwrap();
        return CBalance {
            height: CString::new(balance.height).unwrap().into_raw(),
            total: balance.total,
            orchard: balance.orchard,
            unshielded: balance.unshielded,
        };
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn go_send_txn(
    wallet_name: *const std::os::raw::c_char,
    uuid: *const std::os::raw::c_char,
    address: *const std::os::raw::c_char,
    value: u64,
    target_note_count: usize,
    min_split_output_value: u64,
    memo: *const std::os::raw::c_char,
) -> *mut c_char {
    let rt = tokio::runtime::Runtime::new().expect("Failed to create Tokio runtime");
    unsafe {
        let c_str = std::ffi::CStr::from_ptr(wallet_name);
        let w_str = c_str.to_str().expect("Invalid Utf-8");

        let c_uuid = std::ffi::CStr::from_ptr(uuid);
        let r_uuid = c_uuid.to_str().expect("Invalid Utf-8");

        let c_address = std::ffi::CStr::from_ptr(address);
        let r_address = c_address.to_str().expect("Invalid Address");

        let account_id = Some(Uuid::from_str(r_uuid).expect("wrong UUid"));

        let c_memo = std::ffi::CStr::from_ptr(memo);
        let r_memo = c_memo
            .to_str()
            .map(|m| {
                if m.is_empty() {
                    None
                } else {
                    Some(m.to_owned())
                }
            })
            .expect("Invalid memo");

        let r_target_note_count = if target_note_count == 0 {
            None
        } else {
            Some(target_note_count)
        };

        let r_min_split_output_value = if min_split_output_value == 0 {
            None
        } else {
            Some(min_split_output_value)
        };

        let result = rt.block_on(send_txn(
            w_str.to_string(),
            account_id,
            r_target_note_count,
            r_min_split_output_value,
            r_address.to_owned(),
            value,
            r_memo,
        ));

        match result {
            Ok(txn) => match CString::new(txn) {
                Ok(s) => s.into_raw(),
                Err(_) => return std::ptr::null_mut(),
            },
            Err(e) => {
                println!("Failed to send txn {:?}", e);
                return std::ptr::null_mut();
            }
        }
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn go_get_txn_list(
    ptr: *const std::os::raw::c_char,
    uuid: *const std::os::raw::c_char,
) {
    unsafe {
        let c_ptr_str = std::ffi::CStr::from_ptr(ptr);
        let wallet_dir = c_ptr_str.to_str().expect("Invalid Utf-8");

        let c_uuid_str = std::ffi::CStr::from_ptr(uuid);
        let uuid_str = c_uuid_str.to_str().expect("Invalid Utf-8");

        let account_id = Some(Uuid::from_str(uuid_str).expect("wrong UUid"));
        //let rust_string =
        let result = txn_list(wallet_dir.to_owned(), account_id);

        if result.is_err() {
            println!("Failed to create wallet")
        }
        // Convert Rust String to C string
        /*
        let c_string = match CString::new(rust_string) {
            Ok(s) => s,
            Err(_) => return std::ptr::null_mut(),
        };

        // Transfer ownership to caller
        c_string.into_raw()
        */
    }
}

//~~~~ free memory
#[unsafe(no_mangle)]
pub unsafe extern "C" fn free_string(s: *mut c_char) {
    if s.is_null() {
        return;
    }
    // Reclaim ownership and drop
    unsafe { CString::from_raw(s) };
}

#[unsafe(no_mangle)]
pub extern "C" fn free_struct_array(arr: CAccountArray) {
    unsafe {
        let slice = std::slice::from_raw_parts_mut(arr.ptr, arr.len);
        for item in slice.iter_mut() {
            if !item.uuid.is_null() {
                let _ = CString::from_raw(item.uuid);
            }
            if !item.ufvk.is_null() {
                let _ = CString::from_raw(item.ufvk);
            }
            if !item.uivk.is_null() {
                let _ = CString::from_raw(item.uivk);
            }
            if !item.source.is_null() {
                let _ = CString::from_raw(item.source);
            }
        }
        let _ = Vec::from_raw_parts(arr.ptr, arr.len, arr.len);
    }
}
