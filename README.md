# kraken-pnl-calculator

> ⚠️ DISCLAIMER: This software may contain bugs and is provided "as is" without
> warranty of any kind. The results should be verified independently. This is not
> financial advice. The authors are not responsible for any financial losses or
> tax implications resulting from the use of this tool. Always consult with
> qualified tax professionals for proper financial guidance.

This repository contains the code for the [Kraken](https://pro.kraken.com) FIFO
PnL calculator. This tool is designed to help you calculate your capital gains
tax liability when trading on the Kraken cryptocurrency exchange.

This tool is a kind of supplement to the
[kraken-infinity-grid](https://github.com/btschwertfeger/kraken-infinity-grid)
tool, which is a trading algorithm that trades on the Kraken cryptocurrency
exchange. Since it may be necessary to calculate your capital gains tax
liability when using the algorithm, this tool can help you do that.

## Pre-requisites

You will need to have Rust installed on your system. For more information see
the [Rust website](https://www.rust-lang.org/).

## Example

In order to compute the PnL, the tool fetches the trades and closed orders from
the Kraken API and then calculates the realized and unrealized PnL as well as
the balance (based on the selected time period).

1. Clone the repository and run the following command:

   ```bash
   git clone https:://github.com/btschwertfeger/kraken-pnl-calculator.git
   ```

2. In order to compute the realized and unrealized PnL for the year 2024,
   including orders with the user reference 1734531952, the following command
   can be used:

   ```bash
   $ export KRAKEN_API_KEY=<your-api-key>
   $ export KRAKEN_SECRET_KEY=<your-secret-key>
   $ cargo run -- --symbol XXBTZEUR --userref 1734531952 --year 2024 --start 2024-01-01 --end 2024-12-31 --tier intermediate

   Fetching trades...
   Fetching closed orders...
   ********************************************************************************
   ...
   ********************************************************************************
   Realized PnL: 0.3641678421645933
   Unrealized PnL: 0.08137250483540595
   Balance: 0.00011552000000000002
   Total Buy Volume (Base): 0.00038761
   Total Sell Volume (Base): 0.00027209
   Total Buy Volume (Quote): 35.085761812
   Total Sell Volume (Quote): 25.059991759000003
   Total Cost of Sold Assets: 24.69582391683541
   Total Value of Sold Assets: 25.059991759000003
   ********************************************************************************
   ```

   NOTE: The `--tier` flag is optional and reflects your Kraken account tier,
   which is either `starter`, `immediate`, or `pro`. The default is
   `starter`.
