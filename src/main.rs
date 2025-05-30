/* -*- coding: utf-8 -*-

Copyright (C) 2025 Benjamin Thomas Schwertfeger
GitHub: https://github.com/btschwertfeger

This program computes the FIFO PnL for a given trading pair on the Kraken
exchange. It fetches the trades and closed orders from the Kraken API and
computes the FIFO PnL based on the trades. The program requires the following
environment variables to be set:

- KRAKEN_API_KEY
- KRAKEN_SECRET_KEY

Example:

$ export KRAKEN_API_KEY=mykey
$ export KRAKEN_SECRET_KEY=mysecret
$ cargo run -- --symbol XXBTZEUR --userref 1734531952 --tier pro --year 2024 --start 2024-01-01 --end 2024-12-31
*/

use base64::{engine::general_purpose, Engine as _};
use chrono::{DateTime, Datelike, NaiveDate};
use clap::{Arg, Command};
use hmac::{Hmac, Mac};
use reqwest::blocking::Client;
use serde::Deserialize;
use sha2::{Digest, Sha256, Sha512};
use std::collections::VecDeque;
use std::env;
use std::fs::File;
use std::io::Write;

// =============================================================================
// The following structs are used to fetch historical trades from the Kraken
// API.

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
// The following structs are used to fetch closed orders from the Kraken API.

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

/// A Kraken API client.
struct KrakenAPI {
    api_key: String,
    secret_key: String,
    client: Client,
    base_url: String,
}
impl KrakenAPI {
    /// Creates a new Kraken API client.
    fn new(api_key: String, secret_key: String) -> Self {
        Self {
            api_key,
            secret_key,
            client: Client::new(),
            base_url: "https://api.kraken.com".to_string(),
        }
    }

    /// Computes the Kraken signature for a given request.
    ///
    /// # Arguments
    ///
    /// * `url_path` - The URL path of the API endpoint.
    /// * `data` - The request data to be signed.
    /// * `nonce` - A unique nonce value for the request.
    ///
    /// # Returns
    ///
    /// A string representing the computed Kraken signature.
    ///
    /// # Example
    ///
    /// ```
    /// let signature = api.get_kraken_signature("/0/private/Balance", "nonce=123456", "123456");
    /// ```
    /// The signature as a string.
    ///
    fn get_kraken_signature(&self, url_path: &str, data: &str, nonce: &str) -> String {
        let key = general_purpose::STANDARD.decode(&self.secret_key).unwrap();
        let mut mac = Hmac::<Sha512>::new_from_slice(&key).unwrap();
        mac.update(url_path.as_bytes());
        mac.update(&Sha256::digest(format!("{}{}", nonce, data).as_bytes()));
        general_purpose::STANDARD.encode(mac.finalize().into_bytes())
    }

    /// Sends a POST request to the Kraken API.
    ///
    /// # Returns
    ///
    /// The response as a string.
    ///
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
            .post(format!("{}{}", self.base_url, endpoint))
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

/// Fetches the trades and closed orders from the Kraken API.
///
/// # Arguments
///
/// * `api` - The Kraken API client.
/// * `delay` - The time to wait between requests, depending on the API tier.
/// * `symbol` - The trading pair symbol (e.g., XXBTZEUR).
/// * `userref` - An optional user reference id to filter trades.
/// * `start` - An optional start date for filtering trades.
/// * `end` - An optional end date for filtering trades.
///
/// # Returns
///
/// A vector of trades that match the given criteria.
///
/// This function fetches trades and closed orders from the Kraken API based on
/// the provided criteria. It handles pagination and rate limiting based on the
/// API tier. If a user reference is provided, it also fetches closed orders to
/// match trades with the given user reference. The trades are sorted by time
/// before being returned. All trades that match the given criteria.
///
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

    let mut relevant_trades: Vec<Trade> = Vec::new();
    let mut offset: usize = 0usize;

    println!("Fetching trades...");
    loop {
        let mut paginated_params: Vec<(&str, String)> = params.clone();
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
            relevant_trades.extend(trades);

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
        // When the userref is passed, we need to query the closed orders as
        // well since only those can be matched up with trades based on the user
        // reference number.
        println!("Fetching closed orders...");

        let mut closed_order_txids: Vec<String> = Vec::new();
        offset = 0usize;

        loop {
            let mut paginated_params: Vec<(&str, String)> = params.clone();
            paginated_params.push(("ofs", offset.to_string()));

            let response: String = api.request("/0/private/ClosedOrders", paginated_params.clone());
            let orders_response: OrdersResponse =
                serde_json::from_str(&response).expect("Failed to parse response!");

            if let Some(result) = orders_response.result {
                let orders: Vec<String> = result.closed.into_keys().collect();
                closed_order_txids.extend(orders);

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

        relevant_trades
            .into_iter()
            .filter(|trade| closed_order_txids.contains(&trade.ordertxid))
            .collect()
    } else {
        relevant_trades
    };
    trades.sort_by(|a, b| a.time.partial_cmp(&b.time).unwrap());
    trades
}

/// Computes the FIFO PnL for a given set of trades.
///
/// # Arguments
///
/// * `trades` - A vector of trades to compute the PnL for.
/// * `year` - An optional year to filter the trades. If provided, only profits
///   made within the specified year are considered.
///
/// # Returns
///
/// A tuple containing the realized PnL, unrealized PnL, balance, total buy/sell volumes for base and quote currencies,
/// total cost of sold assets, and total value received from selling them.
///
/// This function processes the trades in a FIFO manner to compute the realized
/// and unrealized PnL. It also calculates the total volume of bought and sold assets for both base and quote currencies,
/// as well as the total cost of sold assets and the total value received from selling them.
fn compute_fifo_pnl(
    trades: Vec<Trade>,
    year: Option<u32>,
) -> (f64, f64, f64, f64, f64, f64, f64, f64, f64) {
    let mut fifo_queue: VecDeque<(f64, f64)> = VecDeque::new();
    let mut realized_pnl: f64 = 0f64;
    let mut balance: f64 = 0f64;
    let mut price: f64 = 0f64;
    let mut total_buy_volume_base: f64 = 0f64;
    let mut total_sell_volume_base: f64 = 0f64;
    let mut total_buy_volume_quote: f64 = 0f64;
    let mut total_sell_volume_quote: f64 = 0f64;
    let mut total_cost_of_sold_assets: f64 = 0f64;
    let mut total_value_of_sold_assets: f64 = 0f64;

    for trade in trades {
        let trade_year: i32 = DateTime::from_timestamp_nanos((trade.time * 1e9) as i64).year();
        let side: String = trade.side;
        let amount: f64 = trade.vol.parse().unwrap();
        price = trade.price.parse().unwrap();
        let fee: f64 = trade.fee.parse().unwrap();

        if side == "buy" {
            let total_cost: f64 = (amount * price) + fee;
            fifo_queue.push_back((amount, total_cost));
            balance += amount;
            total_buy_volume_base += amount;
            total_buy_volume_quote += total_cost;
        } else if side == "sell" {
            let sell_proceeds: f64 = (amount * price) - fee;
            let mut cost_basis: f64 = 0f64;
            let mut base_currency_to_sell: f64 = amount;

            while base_currency_to_sell > 0f64 && !fifo_queue.is_empty() {
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

            let pnl: f64 = sell_proceeds - cost_basis;
            if let Some(year) = year {
                if trade_year == year as i32 {
                    realized_pnl += pnl;
                }
            } else {
                realized_pnl += pnl;
            }
            balance -= amount;
            total_sell_volume_base += amount;
            total_sell_volume_quote += sell_proceeds;
            total_cost_of_sold_assets += cost_basis;
            total_value_of_sold_assets += sell_proceeds;
        }
    }

    let unrealized_pnl: f64 = fifo_queue
        .iter()
        .map(|(lot_amount, lot_cost)| (price - (lot_cost / lot_amount)) * lot_amount)
        .sum();

    (
        realized_pnl,
        unrealized_pnl,
        balance,
        total_buy_volume_base,
        total_sell_volume_base,
        total_buy_volume_quote,
        total_sell_volume_quote,
        total_cost_of_sold_assets,
        total_value_of_sold_assets,
    )
}

/// Writes the trades to a CSV file.
///
/// # Arguments
///
/// * `trades` - A reference to a vector of trades to be written to the CSV
///   file.
/// * `file_path` - The path of the CSV file to write the trades to.
///
/// This function writes the trades to a CSV file with the specified file path.
/// The CSV file includes a header row and each trade is written as a row in the
/// CSV file. The time field is converted to a human-readable format before
/// being written to the file.
fn write_trades_to_csv(trades: &Vec<Trade>, file_path: &str) {
    let mut file: File = File::create(file_path).expect("Could not create file");
    writeln!(
        file,
        "time,pair,side,price,fee,vol,cost,ordertype,ordertxid"
    )
    .expect("Failed to write header to CSV!");

    for trade in trades {
        let time_str = DateTime::from_timestamp_nanos((trade.time * 1e9) as i64)
            .format("%Y-%m-%d %H:%M:%S")
            .to_string();
        writeln!(
            file,
            "{},{},{},{},{},{},{},{},{}",
            time_str,
            trade.pair,
            trade.side,
            trade.price,
            trade.fee,
            trade.vol,
            trade.cost,
            trade.ordertype,
            trade.ordertxid,
        )
        .expect("Failed to write trades to CSV!");
    }
}

// =============================================================================

fn main() {
    let matches = Command::new("FIFO PnL Calculator")
        .version("0.1.0")
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
                .action(clap::ArgAction::SetTrue),
        )
        .arg(
            Arg::new("year")
                .long("year")
                .value_name("YEAR")
                .help("Only consider profits made within a specific year")
                .value_parser(clap::value_parser!(u32)),
        )
        .arg(
            Arg::new("tier")
                .long("tier")
                .value_name("TIER")
                .help("API tier (starter, intermediate, or pro)")
                .required(true)
                .value_parser(clap::value_parser!(String)),
        )
        .get_matches();

    let symbol: &String = matches.get_one::<String>("symbol").unwrap();
    let year: Option<u32> = matches.get_one::<u32>("year").copied();
    let start: Option<f64> = matches.get_one::<String>("start").map(|s| {
        NaiveDate::parse_from_str(s, "%Y-%m-%d")
            .unwrap()
            .and_hms_opt(0, 0, 0)
            .unwrap()
            .and_utc()
            .timestamp() as f64
    });
    let end: Option<f64> = matches.get_one::<String>("end").map(|s| {
        NaiveDate::parse_from_str(s, "%Y-%m-%d")
            .unwrap()
            .and_hms_opt(23, 59, 59)
            .unwrap()
            .and_utc()
            .timestamp() as f64
    });
    let userref: Option<i32> = matches.get_one::<i32>("userref").copied();
    let csv = matches.get_flag("csv");
    let api_key: String =
        env::var("KRAKEN_API_KEY").expect("The environment variable 'KRAKEN_API_KEY' must be set!");
    let secret_key: String = env::var("KRAKEN_SECRET_KEY")
        .expect("The environment variable 'KRAKEN_SECRET_KEY' must be set!");

    let api = KrakenAPI::new(api_key, secret_key);
    let delay: u64 = match matches.get_one::<String>("tier").unwrap().as_str() {
        "starter" => 7, // It takes 7 seconds to recover 2 API points with 0.33 points per second.
        "intermediate" => 4, // It takes 4 seconds to recover 2 API points with 0.5 points per second.
        "pro" => 2,          // It takes 2 seconds to recover 2 API points with 1 point per second.
        _ => 7,              // Default to starter tier.
    };

    // =========================================================================
    // Fetch trades and compute FIFO PnL
    let trades = fetch_trades(api, delay, symbol, userref, start, end);

    if csv {
        write_trades_to_csv(&trades, "trades.csv");
    }

    println!("{}", "*".repeat(80));
    for trade in &trades {
        println!(
            "{:?} {}",
            trade,
            DateTime::from_timestamp_nanos((trade.time * 1e9) as i64).format("%Y-%m-%d %H:%M:%S")
        );
    }

    // =========================================================================
    // Compute FIFO PnL
    println!("{}", "*".repeat(80));
    let (
        realized_pnl,
        unrealized_pnl,
        balance,
        total_buy_volume_base,
        total_sell_volume_base,
        total_buy_volume_quote,
        total_sell_volume_quote,
        total_cost_of_sold_assets,
        total_value_of_sold_assets,
    ) = compute_fifo_pnl(trades, year);

    // =========================================================================
    println!("Realized PnL: {}", realized_pnl);
    println!("Unrealized PnL: {}", unrealized_pnl);
    println!("Balance: {}", balance);
    println!("Total Buy Volume (Base): {}", total_buy_volume_base);
    println!("Total Sell Volume (Base): {}", total_sell_volume_base);
    println!("Total Buy Volume (Quote): {}", total_buy_volume_quote);
    println!("Total Sell Volume (Quote): {}", total_sell_volume_quote);
    println!("Total Cost of Sold Assets: {}", total_cost_of_sold_assets);
    println!("Total Value of Sold Assets: {}", total_value_of_sold_assets);
    println!("{}", "*".repeat(80));
    // =========================================================================
}
