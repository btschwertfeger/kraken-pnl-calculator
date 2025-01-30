# kraken-fifo-tax

This repository contains the code for the Kraken FIFO tax calculator. This tool
is designed to help you calculate your capital gains tax liability when trading
on the Kraken cryptocurrency exchange.

This tool is a kind of supplement to the
[kraken-infinity-grid](https://github.com/btschwertfeger/kraken-infinity-grid)
tool, which is a trading algorithm that trades on the Kraken cryptocurrency
exchange. Since it may be necessary to calculate your capital gains tax
liability when using the algorithm, this tool can help you do that.

## Pre-requisites

You will need to have Rust installed on your system. For more information see
the [Rust website](https://www.rust-lang.org/).

## Usage

1. Clone the repository and run the following command:

   ```bash
   git clone https:://github.com/btschwertfeger/kraken-fifo-tax.git
   ```

2. After setting the API keys via environment variables, the tool can be run
    with the following command:

   ```bash
   export KRAKEN_API_KEY=<your-api-key>
   export KRAKEN_SECRET_KEY=<your-secret-key>
   cargo run -- --symbol XXBTZEUR --userref 1734531952 --start "2024-01-01" --tier intermediate
   ```

   NOTE: The `--tier` flag is optional and reflects your Kraken account tier,
         which is either `starter`, `immediate`, or `pro`. The default is
         `starter`.

## Example output

In order to compute the tax liability, the tool fetches the trades and closed
orders from the Kraken API and then calculates the realized and unrealized PnL
as well as the balance (based on the selected time period).

In order to compute the realized and unrealized PnL for the year 2025, including
orders with the user reference 1734531952, the following command can be used:

```bash
$ export KRAKEN_API_KEY=mykey
$ export KRAKEN_SECRET_KEY=mysecret
$ cargo run -- --symbol XXBTZEUR --userref 1734531952 --year 2025

Fetching trades...
Fetching closed orders...
********************************************************************************
...
********************************************************************************
Realized PnL: 3.4544660313202176
Unrealized PnL: 0.3216477636798123
Balance: 0.0010760599999999996
********************************************************************************
```