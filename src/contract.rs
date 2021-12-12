use anyhow::Result;
#[cfg(feature = "std")]
use cargo_contract::cmd::CallCommand;
use cargo_contract::ExtrinsicOpts;
use std::str::FromStr;

use crate::service::ApronService;

// execute contract private key
const SURI: &str = "0xe40891ed4fa2eb6b8b89b1d641ae72e8c1ba383d809eeba64131b37bf0aa3898";
const GAS_LIMIT: u64 = 50000000000;

pub fn submit_usage(
    ws_endpoint: String,
    stats_contract_addr: String,
    stats_contract_abi: String,
    args: Vec<String>,
) {
    let result = exec(
        ws_endpoint,
        stats_contract_addr,
        stats_contract_abi,
        String::from("submit_usage"),
        args,
        // vec![
        //     format!("\"{}\"", uuid),    //service_uuid
        //     String::from("0"),          //nonce
        //     format!("\"{}\"", "test1"), //user_key
        //     String::from("12345678"),   //start_time
        //     String::from("12345678"),   //end_time
        //     String::from("12345678"),   //usage
        //     format!("\"{}\"", "test1"), //price_plan
        //     String::from("12345678"),   //cost
        // ],
    );
    println!("result: {:?}", result);
    assert!(result.is_ok());
    match result {
        Ok(r) => {
            println!("exec result: {}", r)
        }
        Err(e) => {
            println!("exec err: {}", e)
        }
    }
}

pub fn add_service(
    ws_endpoint: String,
    market_contract_addr: String,
    market_contract_abi: String,
    args: Vec<String>,
) {
    println!("add_service");
    let result = exec(
        ws_endpoint,
        market_contract_addr,
        market_contract_abi,
        String::from("add_service"),
        args,
    );
    println!("result: {:?}", result);
    assert!(result.is_ok());
    match result {
        Ok(r) => {
            println!("exec result: {}", r)
        }
        Err(e) => {
            println!("exec err: {}", e)
        }
    }
}

pub fn call(
    ws_endpoint: String,
    contract_addr: String,
    abi_path: String,
    name: String,
    args: Vec<String>,
) -> Result<String> {
    let url = url::Url::from_str(&ws_endpoint).unwrap();
    let call = CallCommand {
        name,
        args,
        extrinsic_opts: ExtrinsicOpts {
            url,
            suri: SURI.to_string(),
            password: None,
            verbosity: Default::default(),
        },
        gas_limit: GAS_LIMIT,
        value: 0,
        contract: contract_addr,
        rpc: true,
        path: abi_path,
    };
    call.run()
}

pub fn exec(
    ws_endpoint: String,
    contract_addr: String,
    abi_path: String,
    name: String,
    args: Vec<String>,
) -> Result<String> {
    let url = url::Url::from_str(&ws_endpoint).unwrap();
    let call = CallCommand {
        name,
        args,
        extrinsic_opts: ExtrinsicOpts {
            url,
            suri: SURI.to_string(),
            password: None,
            verbosity: Default::default(),
        },
        gas_limit: GAS_LIMIT,
        value: 0,
        contract: contract_addr,
        rpc: false,
        path: abi_path,
    };
    call.run()
}
