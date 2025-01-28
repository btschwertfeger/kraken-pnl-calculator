/* -*- coding: utf-8 -*-

Copyright (C) 2025 Benjamin Thomas Schwertfeger
GitHub: https://github.com/btschwertfeger

TODOs:
- Cache trades and orders to avoid fetching them multiple times
*/

use base64::{engine::general_purpose, Engine as _};
use chrono::NaiveDate;
use clap::{Arg, Command};
use hmac::{Hmac, Mac};
use reqwest::blocking::Client;
use serde::Deserialize;
use serde_json;
use serde_urlencoded;
use sha2::{Digest, Sha256, Sha512};
use std::collections::VecDeque;
use std::env;

#[derive(Deserialize, Debug)]
#[allow(dead_code)]
struct Trade {
    ordertxid: String,
    pair: String,
    time: f64,
    #[serde(rename = "type")]
    side: String,
    price: String,
    fee: String,
    vol: String,
    cost: String,
    ordertype: String,
}

#[derive(Deserialize, Debug)]
struct TradesResult {
    trades: std::collections::HashMap<String, Trade>,
    count: u32,
}

#[derive(Deserialize, Debug)]
struct TradesResponse {
    error: Vec<String>,
    result: Option<TradesResult>,
}

// =============================================================================

#[derive(Deserialize, Debug)]
struct Order {}

#[derive(Deserialize, Debug)]
struct OrdersResult {
    closed: std::collections::HashMap<String, Order>,
    count: u32,
}

#[derive(Deserialize, Debug)]
struct OrdersResponse {
    error: Vec<String>,
    result: Option<OrdersResult>,
}

// =============================================================================

struct KrakenAPI {
    api_key: String,
    secret_key: String,
    client: Client,
    base_url: String,
}
impl KrakenAPI {
    fn new(api_key: String, secret_key: String) -> Self {
        Self {
            api_key: api_key,
            secret_key: secret_key,
            client: Client::new(),
            base_url: "https://api.kraken.com".to_string(),
        }
    }
    fn get_kraken_signature(&self, url_path: &str, data: &str, nonce: &str) -> String {
        let key = general_purpose::STANDARD.decode(&self.secret_key).unwrap();
        let mut mac = Hmac::<Sha512>::new_from_slice(&key).unwrap();
        mac.update(url_path.as_bytes());
        mac.update(&Sha256::digest(format!("{}{}", nonce, data).as_bytes()));
        general_purpose::STANDARD.encode(mac.finalize().into_bytes())
    }

    fn request(&self, endpoint: &str, params: Vec<(&str, String)>) -> String {
        let nonce = format!(
            "{}",
            (chrono::Utc::now().timestamp_nanos_opt().unwrap() / 10)
        );
        let mut params = params.clone();
        params.push(("nonce", nonce.clone()));
        let encoded_params = serde_urlencoded::to_string(&params).unwrap();
        let response = self
            .client
            .post(&format!("{}{}", self.base_url, endpoint))
            .header(
                "Content-Type",
                "application/x-www-form-urlencoded; charset=utf-8",
            )
            .header("API-Key", &self.api_key)
            .header(
                "API-Sign",
                self.get_kraken_signature(endpoint, &encoded_params, &nonce),
            )
            .form(&params)
            .send()
            .expect("Failed to send POST request!");

        if response.status().is_success() {
            response.text().expect("Failed to read response text!")
        } else {
            eprintln!("Error during request: {}", response.status());
            "".to_string()
        }
    }
}

// =============================================================================

fn fetch_trades(
    api: KrakenAPI,
    delay: u64,
    symbol: &String,
    userref: Option<i32>,
    start: Option<f64>,
    end: Option<f64>,
) -> Vec<Trade> {
    let mut params = vec![];

    if let Some(userref) = userref {
        params.push(("userref", userref.to_string()));
    }
    if let Some(start) = start {
        params.push(("start", start.to_string()));
    }
    if let Some(end) = end {
        params.push(("end", end.to_string()));
    }

    let mut all_trades: Vec<Trade> = Vec::new();
    let mut offset = 0;

    println!("Fetching trades...");
    loop {
        let mut paginated_params = params.clone();
        paginated_params.push(("ofs", offset.to_string()));

        let response: String = api.request("/0/private/TradesHistory", paginated_params.clone());
        let trades_response: TradesResponse =
            serde_json::from_str(&response).expect("Failed to parse response!");

        if let Some(result) = trades_response.result {
            let trades: Vec<Trade> = result
                .trades
                .into_iter()
                .filter(|(_, trade)| trade.pair == *symbol)
                .map(|(_, trade)| trade)
                .collect();
            all_trades.extend(trades);

            println!("Fetched {}/{} trades...", all_trades.len(), result.count);
            if result.count as usize <= offset + 50 {
                break;
            }
            std::thread::sleep(std::time::Duration::from_secs(delay));
        } else {
            eprintln!("Error fetching trades: {:?}", trades_response.error);
            std::process::exit(1);
        }

        offset += 50;
    }

    // =========================================================================
    let mut trades: Vec<Trade> = if userref.is_some() {
        println!("Fetching closed orders...");

        let mut closed_order_txids: Vec<String> = Vec::new();
        offset = 0;

        loop {
            let mut paginated_params: Vec<(&str, String)> = params.clone();
            paginated_params.push(("ofs", offset.to_string()));

            let response: String = api.request("/0/private/ClosedOrders", paginated_params.clone());
            let orders_response: OrdersResponse =
                serde_json::from_str(&response).expect("Failed to parse response!");

            if let Some(result) = orders_response.result {
                let orders: Vec<String> = result.closed.into_iter().map(|(txid, _)| txid).collect();
                closed_order_txids.extend(orders);

                println!(
                    "Fetched {}/{} closed orders...",
                    closed_order_txids.len(),
                    result.count
                );
                if result.count as usize <= closed_order_txids.len() {
                    break;
                }
                std::thread::sleep(std::time::Duration::from_secs(delay));
            } else {
                eprintln!("Error fetching closed orders: {:?}", orders_response.error);
                std::process::exit(1);
            }

            offset += 50;
        }

        all_trades
            .into_iter()
            .filter(|trade| closed_order_txids.contains(&trade.ordertxid))
            .collect()
    } else {
        all_trades
    };
    trades.sort_by(|a, b| a.time.partial_cmp(&b.time).unwrap());
    trades
}

fn compute_fifo_pnl(trades: Vec<Trade>) -> (f64, f64, f64) {
    let mut fifo_queue: VecDeque<(f64, f64)> = VecDeque::new();
    let mut realized_pnl: f64 = 0f64;
    let mut balance: f64 = 0f64;
    let mut price: f64 = 0f64;

    for trade in trades {
        let side: String = trade.side;
        let amount: f64 = trade.vol.parse().unwrap();
        price = trade.price.parse().unwrap();
        let fee: f64 = trade.fee.parse().unwrap();

        if side == "buy" {
            let total_cost = (amount * price) + fee;
            fifo_queue.push_back((amount, total_cost));
            balance += amount;
        } else if side == "sell" {
            let sell_proceeds: f64 = (amount * price) - fee;
            let mut cost_basis: f64 = 0f64;
            let mut base_currency_to_sell = amount;

            while base_currency_to_sell > 0.0 && !fifo_queue.is_empty() {
                let (fifo_amount, fifo_cost) = fifo_queue.pop_front().unwrap();
                if fifo_amount <= base_currency_to_sell {
                    cost_basis += fifo_cost;
                    base_currency_to_sell -= fifo_amount;
                } else {
                    let partial_cost: f64 = (fifo_cost / fifo_amount) * base_currency_to_sell;
                    cost_basis += partial_cost;
                    fifo_queue.push_front((
                        fifo_amount - base_currency_to_sell,
                        fifo_cost - partial_cost,
                    ));
                    base_currency_to_sell = 0f64;
                }
            }

            let pnl = sell_proceeds - cost_basis;
            realized_pnl += pnl;
            balance -= amount;
        }
    }

    let unrealized_pnl = fifo_queue
        .iter()
        .map(|(lot_amount, lot_cost)| (price - (lot_cost / lot_amount)) * lot_amount)
        .sum();

    (realized_pnl, unrealized_pnl, balance)
}

fn main() {
    let matches = Command::new("FIFO PnL Calculator")
        .version("1.0")
        .author("Benjamin Thomas Schwertfeger")
        .about("Compute FIFO PnL for Kraken trades")
        .arg(
            Arg::new("symbol")
                .long("symbol")
                .value_name("SYMBOL")
                .help("Trading pair symbol (e.g., XXBTZEUR)")
                .required(true)
                .value_parser(clap::value_parser!(String)),
        )
        .arg(
            Arg::new("start")
                .long("start")
                .value_name("START")
                .help("Start date for filtering trades (e.g., 2023-01-01)")
                .value_parser(clap::value_parser!(String)),
        )
        .arg(
            Arg::new("end")
                .long("end")
                .value_name("END")
                .help("End date for filtering trades (e.g., 2023-12-31)")
                .value_parser(clap::value_parser!(String)),
        )
        .arg(
            Arg::new("userref")
                .long("userref")
                .value_name("USERREF")
                .help("A user reference id to filter trades")
                .value_parser(clap::value_parser!(i32)),
        )
        .arg(
            Arg::new("csv")
                .long("csv")
                .help("Generate a CSV file listing the trades")
                .value_parser(clap::value_parser!(bool)),
        )
        .arg(
            Arg::new("tier")
                .long("tier")
                .value_name("TIER")
                .help("API tier (starter, intermediate, pro)")
                .required(true)
                .value_parser(clap::value_parser!(String)),
        )
        .get_matches();

    let symbol = matches.get_one::<String>("symbol").unwrap();
    let start = matches.get_one::<String>("start").map(|s| {
        NaiveDate::parse_from_str(s, "%Y-%m-%d")
            .unwrap()
            .and_hms_opt(0, 0, 0)
            .unwrap()
            .and_utc()
            .timestamp() as f64
    });
    let end = matches.get_one::<String>("end").map(|s| {
        NaiveDate::parse_from_str(s, "%Y-%m-%d")
            .unwrap()
            .and_hms_opt(23, 59, 59)
            .unwrap()
            .and_utc()
            .timestamp() as f64
    });
    let userref = matches.get_one::<i32>("userref").copied();
    let api_key = env::var("KRAKEN_API_KEY").expect("KRAKEN_API_KEY must be set");
    let secret_key = env::var("KRAKEN_SECRET_KEY").expect("KRAKEN_SECRET_KEY must be set");

    let api = KrakenAPI::new(api_key, secret_key);
    let delay: u64 = match matches.get_one::<String>("tier").unwrap().as_str() {
        "starter" => 7,
        "intermediate" => 4,
        "pro" => 2,
        _ => 7,
    };
    let trades = fetch_trades(api, delay, symbol, userref, start, end);

    println!("{}", "*".repeat(80));
    for trade in &trades {
        println!("{:?}", trade);
    }

    println!("{}", "*".repeat(80));
    let (realized_pnl, unrealized_pnl, balance) = compute_fifo_pnl(trades);

    println!("Realized PnL: {}", realized_pnl);
    println!("Unrealized PnL: {}", unrealized_pnl);
    println!("Balance: {}", balance);
    println!("{}", "*".repeat(80));
}
