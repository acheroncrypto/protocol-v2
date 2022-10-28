use crate::math::constants::{
    AMM_RESERVE_PRECISION, PRICE_PRECISION, PRICE_PRECISION_U64, QUOTE_PRECISION,
};
use crate::math::funding::*;
use crate::state::oracle::HistoricalOracleData;
use crate::state::perp_market::{PerpMarket, AMM};

#[test]
fn capped_sym_funding_test() {
    // more shorts than longs, positive funding, 1/3 of fee pool too small
    let mut market = PerpMarket {
        amm: AMM {
            base_asset_reserve: 512295081967,
            quote_asset_reserve: 488 * AMM_RESERVE_PRECISION,
            sqrt_k: 500 * AMM_RESERVE_PRECISION,
            peg_multiplier: 50000000,
            base_asset_amount_with_amm: -12295081967,
            base_asset_amount_long: 12295081967,
            base_asset_amount_short: -12295081967 * 2,
            total_exchange_fee: QUOTE_PRECISION / 2,
            total_fee_minus_distributions: (QUOTE_PRECISION as i128) / 2,

            last_mark_price_twap: 50 * PRICE_PRECISION_U64,
            historical_oracle_data: HistoricalOracleData {
                last_oracle_price_twap: (49 * PRICE_PRECISION) as i64,

                ..HistoricalOracleData::default()
            },
            funding_period: 3600,

            ..AMM::default()
        },
        ..PerpMarket::default()
    };

    let balanced_funding = calculate_funding_rate(
        market.amm.last_mark_price_twap as u128,
        market.amm.historical_oracle_data.last_oracle_price_twap as i128,
        market.amm.funding_period,
    )
    .unwrap();

    assert_eq!(balanced_funding, 41666666);

    let (long_funding, short_funding, _) =
        calculate_funding_rate_long_short(&mut market, balanced_funding).unwrap();

    assert_eq!(long_funding, balanced_funding);
    assert!(long_funding > short_funding);
    assert_eq!(short_funding, 24222164);

    // only spend 1/3 of fee pool, ((.5-.416667)) * 3 < .25
    assert_eq!(market.amm.total_fee_minus_distributions, 416667);

    // more longs than shorts, positive funding, amm earns funding
    market = PerpMarket {
        amm: AMM {
            base_asset_reserve: 512295081967,
            quote_asset_reserve: 488 * AMM_RESERVE_PRECISION,
            sqrt_k: 500 * AMM_RESERVE_PRECISION,
            peg_multiplier: 50000000,
            base_asset_amount_with_amm: 12295081967,
            base_asset_amount_long: 12295081967 * 2,
            base_asset_amount_short: -12295081967,
            total_exchange_fee: QUOTE_PRECISION / 2,
            total_fee_minus_distributions: (QUOTE_PRECISION as i128) / 2,
            last_mark_price_twap: 50 * PRICE_PRECISION_U64,
            historical_oracle_data: HistoricalOracleData {
                last_oracle_price_twap: (49 * PRICE_PRECISION) as i64,

                ..HistoricalOracleData::default()
            },
            funding_period: 3600,

            ..AMM::default()
        },
        ..PerpMarket::default()
    };

    assert_eq!(balanced_funding, 41666666);

    let (long_funding, short_funding, _) =
        calculate_funding_rate_long_short(&mut market, balanced_funding).unwrap();

    assert_eq!(long_funding, balanced_funding);
    assert_eq!(long_funding, short_funding);
    let new_fees = market.amm.total_fee_minus_distributions;
    assert!(new_fees > QUOTE_PRECISION as i128 / 2);
    assert_eq!(new_fees, 1012295); // made over $.50
}

#[test]
fn funding_unsettled_lps_amm_win_test() {
    // more shorts than longs, positive funding

    // positive base_asset_amount_with_unsettled_lp =
    // 1) lots of long users who have lp as counterparty
    // 2) the lps should be short but its unsettled...
    // 3) amm takes on the funding revenu/cost of those short LPs

    let mut market = PerpMarket {
        amm: AMM {
            base_asset_reserve: 512295081967,
            quote_asset_reserve: 488 * AMM_RESERVE_PRECISION,
            sqrt_k: 500 * AMM_RESERVE_PRECISION,
            peg_multiplier: 50000000,
            base_asset_amount_with_amm: -12295081967, //~12
            base_asset_amount_long: 12295081967,
            base_asset_amount_short: -12295081967 * 2,
            base_asset_amount_with_unsettled_lp: (AMM_RESERVE_PRECISION * 500) as i128, //wowsers
            total_exchange_fee: QUOTE_PRECISION / 2,
            total_fee_minus_distributions: (QUOTE_PRECISION as i128) / 2,

            last_mark_price_twap: 50 * PRICE_PRECISION_U64,
            historical_oracle_data: HistoricalOracleData {
                last_oracle_price_twap: (49 * PRICE_PRECISION) as i64,

                ..HistoricalOracleData::default()
            },
            funding_period: 3600,

            ..AMM::default()
        },
        ..PerpMarket::default()
    };

    let balanced_funding = calculate_funding_rate(
        market.amm.last_mark_price_twap as u128,
        market.amm.historical_oracle_data.last_oracle_price_twap as i128,
        market.amm.funding_period,
    )
    .unwrap();

    assert_eq!(balanced_funding, 41666666);
    assert_eq!(market.amm.total_fee_minus_distributions, 500000);

    let (long_funding, short_funding, _) =
        calculate_funding_rate_long_short(&mut market, balanced_funding).unwrap();

    let settled_net_market_position = market
        .amm
        .base_asset_amount_with_amm
        .checked_add(market.amm.base_asset_amount_with_unsettled_lp)
        .unwrap();

    let net_market_position_funding_payment =
        calculate_funding_payment_in_quote_precision(balanced_funding, settled_net_market_position)
            .unwrap();
    let uncapped_funding_pnl = -net_market_position_funding_payment;

    assert_eq!(market.amm.base_asset_amount_with_amm, -12295081967);
    assert_eq!(market.amm.base_asset_amount_with_unsettled_lp, 500000000000);
    assert_eq!(settled_net_market_position, 487704918033);
    assert_eq!(net_market_position_funding_payment, -20321037);
    assert_eq!(uncapped_funding_pnl, 20321037); //protocol revenue

    assert_eq!(long_funding, balanced_funding);
    assert_eq!(short_funding, balanced_funding);

    assert!(long_funding == short_funding);

    // making money off unsettled lps
    assert_eq!(market.amm.total_fee_minus_distributions, 20821037);

    // more longs than shorts, positive funding, amm earns funding
    market = PerpMarket {
        amm: AMM {
            base_asset_reserve: 512295081967,
            quote_asset_reserve: 488 * AMM_RESERVE_PRECISION,
            sqrt_k: 500 * AMM_RESERVE_PRECISION,
            peg_multiplier: 50000000,
            base_asset_amount_with_amm: 12295081967,
            base_asset_amount_long: 12295081967 * 2,
            base_asset_amount_short: -12295081967,
            base_asset_amount_with_unsettled_lp: (AMM_RESERVE_PRECISION * 500) as i128, //wowsers
            total_exchange_fee: QUOTE_PRECISION / 2,
            total_fee_minus_distributions: (QUOTE_PRECISION as i128) / 2,
            last_mark_price_twap: 50 * PRICE_PRECISION_U64,
            historical_oracle_data: HistoricalOracleData {
                last_oracle_price_twap: (49 * PRICE_PRECISION) as i64,

                ..HistoricalOracleData::default()
            },
            funding_period: 3600,

            ..AMM::default()
        },
        ..PerpMarket::default()
    };

    let balanced_funding = calculate_funding_rate(
        market.amm.last_mark_price_twap as u128,
        market.amm.historical_oracle_data.last_oracle_price_twap as i128,
        market.amm.funding_period,
    )
    .unwrap();
    assert_eq!(balanced_funding, 41666666);

    let (long_funding, short_funding, drift_pnl) =
        calculate_funding_rate_long_short(&mut market, balanced_funding).unwrap();

    assert_eq!(drift_pnl, 21345628);
    assert_eq!(long_funding, balanced_funding);
    assert_eq!(long_funding, short_funding);
    let new_fees = market.amm.total_fee_minus_distributions;
    assert!(new_fees > QUOTE_PRECISION as i128 / 2);
    assert_eq!(new_fees, 21845628); // made more
}

#[test]
fn funding_unsettled_lps_amm_lose_test() {
    // more shorts than longs, positive funding

    // positive base_asset_amount_with_unsettled_lp =
    // 1) lots of long users who have lp as counterparty
    // 2) the lps should be short but its unsettled...
    // 3) amm takes on the funding revenu/cost of those short LPs

    let mut market = PerpMarket {
        amm: AMM {
            base_asset_reserve: 512295081967,
            quote_asset_reserve: 488 * AMM_RESERVE_PRECISION,
            sqrt_k: 500 * AMM_RESERVE_PRECISION,
            peg_multiplier: 50000000,
            base_asset_amount_with_amm: -12295081967, //~12
            base_asset_amount_long: 12295081967,
            base_asset_amount_short: -12295081967 * 2,
            base_asset_amount_with_unsettled_lp: -((AMM_RESERVE_PRECISION * 500) as i128), //wowsers
            total_exchange_fee: QUOTE_PRECISION / 2,
            total_fee_minus_distributions: ((QUOTE_PRECISION * 99999) as i128),

            last_mark_price_twap: 50 * PRICE_PRECISION_U64,
            historical_oracle_data: HistoricalOracleData {
                last_oracle_price_twap: (49 * PRICE_PRECISION) as i64,

                ..HistoricalOracleData::default()
            },
            funding_period: 3600,

            ..AMM::default()
        },
        ..PerpMarket::default()
    };

    let balanced_funding = calculate_funding_rate(
        market.amm.last_mark_price_twap as u128,
        market.amm.historical_oracle_data.last_oracle_price_twap as i128,
        market.amm.funding_period,
    )
    .unwrap();

    assert_eq!(balanced_funding, 41666666);
    assert_eq!(market.amm.total_fee_minus_distributions, 99999000000);

    let (long_funding, short_funding, _) =
        calculate_funding_rate_long_short(&mut market, balanced_funding).unwrap();

    let settled_net_market_position = market
        .amm
        .base_asset_amount_with_amm
        .checked_add(market.amm.base_asset_amount_with_unsettled_lp)
        .unwrap();

    let net_market_position_funding_payment =
        calculate_funding_payment_in_quote_precision(balanced_funding, settled_net_market_position)
            .unwrap();
    let uncapped_funding_pnl = -net_market_position_funding_payment;

    assert_eq!(market.amm.base_asset_amount_with_amm, -12295081967);
    assert_eq!(
        market.amm.base_asset_amount_with_unsettled_lp,
        -500000000000
    );
    assert_eq!(settled_net_market_position, -512295081967);
    assert_eq!(net_market_position_funding_payment, 21345628);
    assert_eq!(uncapped_funding_pnl, -21345628); //protocol loses $21

    assert_eq!(long_funding, balanced_funding);
    assert_eq!(short_funding, balanced_funding);

    assert!(long_funding == short_funding);

    // making money off unsettled lps
    assert_eq!(market.amm.total_fee_minus_distributions, 99977654372);

    // more longs than shorts, positive funding, amm earns funding
    market = PerpMarket {
        amm: AMM {
            base_asset_reserve: 512295081967,
            quote_asset_reserve: 488 * AMM_RESERVE_PRECISION,
            sqrt_k: 500 * AMM_RESERVE_PRECISION,
            peg_multiplier: 50000000,
            base_asset_amount_with_amm: 12295081967,
            base_asset_amount_long: 12295081967 * 2,
            base_asset_amount_short: -12295081967,
            base_asset_amount_with_unsettled_lp: -((AMM_RESERVE_PRECISION * 500) as i128), //wowsers
            total_exchange_fee: QUOTE_PRECISION / 2,
            total_fee_minus_distributions: (QUOTE_PRECISION as i128) / 2,
            last_mark_price_twap: 50 * PRICE_PRECISION_U64,
            historical_oracle_data: HistoricalOracleData {
                last_oracle_price_twap: (49 * PRICE_PRECISION) as i64,

                ..HistoricalOracleData::default()
            },
            funding_period: 3600,

            ..AMM::default()
        },
        ..PerpMarket::default()
    };

    let balanced_funding = calculate_funding_rate(
        market.amm.last_mark_price_twap as u128,
        market.amm.historical_oracle_data.last_oracle_price_twap as i128,
        market.amm.funding_period,
    )
    .unwrap();
    assert_eq!(balanced_funding, 41666666);

    let (long_funding, short_funding, drift_pnl) =
        calculate_funding_rate_long_short(&mut market, balanced_funding).unwrap();

    assert_eq!(drift_pnl, -20321037);
    assert_eq!(long_funding, balanced_funding);
    assert_eq!(short_funding, 90110989);
    assert_eq!(long_funding < short_funding, true);

    let new_fees = market.amm.total_fee_minus_distributions;
    assert_eq!(new_fees, 416667); // lost
}