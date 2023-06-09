use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::Duration;

use aws_sdk_cloudwatch::model::{ComparisonOperator, MetricAlarm, Statistic};
use aws_sdk_cloudwatch::{Client as cloudwatchClient, Error, PKG_VERSION};
use aws_sdk_sts::Client as stsClient;
use clap::{Arg, Command};
use tokio::fs;

#[derive(Deserialize, Debug)]
struct AccountsConfig {
    account: Vec<AccountConfig>,
}

#[derive(Deserialize, Debug)]
struct AccountConfig {
    namespace: String,
    region: String,
    role_arn: String,
}

#[derive(Debug)]
struct GetWidgetProps {
    app_name: String,
    end: String,
    period: String,
    region: Option<String>,
    role_arn: String,
    start: String,
    template_path: PathBuf,
    title: String,
    verbose: bool,
}

#[derive(Serialize, Debug)]
struct MetricAlarmDetails {
    program_name: String,
    alarm_name: String,
    alarm_arn: String,
    alarm_description: String,
    dimensions: Vec<String>,
    actions_enabled: bool,
    period: i32,
    threshold: f64,
    comparison_operator: String,
    treat_missing_data: String,
    statistic: String,
}

#[derive(Debug)]
struct DescribeAlarmsProps {
    region: Option<String>,
    role_arn: String,
    verbose: bool,
}

pub mod aws_regions {

    pub trait AWSRegionName {
        fn name(self: Self) -> &'static str;
    }

    impl AWSRegionName for AirportCode {
        fn name(self: Self) -> &'static str {
            match self {
                AirportCode::IAD => "us-east-1",
                AirportCode::PDX => "us-west-2",
                AirportCode::DUB => "eu-west-1",
            }
        }
    }

    // this is dumb, not sure how to force provide a static string in this case though...
    pub fn convert_to_name(region: &str) -> &'static str {
        match region {
            "us-east-1" => "us-east-1",
            "us-west-2" => "us-west-2",
            "eu-west-1" => "eu-west-1",
            _ => "us-west-2",
        }
    }

    /// AirportCode enum represents the 3-letter international airport code closest to a data center region
    #[derive(Debug, Copy, Clone)]
    pub enum AirportCode {
        IAD,
        PDX,
        DUB,
    }
}

/// Dev CLI for repetitive AWS account tasks
///
/// ## Accounts Config
///
/// The accounts are defined in [TOML](https://toml.io) syntax. The file should be a list of tables containing `namespace`, `account_id`, and `region` for each account.
///
/// Example (from the repo's accounts.toml):
///
/// ```toml
/// [[account]]
/// namespace = "SomeDataProcessingProgram"
/// account_id = "111111111111"
/// region = "us-east-1"
/// ```
///
/// To validate accounts config is parsed properly:
///
/// ```bash
/// cargo run -- config <ACCOUNT.TOML FILE>
///
/// # example
/// cargo run -- config accounts.toml
/// AccountConfig { namespace: "SomeDataProcessingProgram", account_id: "111111111111", region: "us-east-1" }
/// AccountConfig { namespace: "SomeDataProcessingProgram", account_id: "222222222222", region: "eu-west-1" }
/// AccountConfig { namespace: "SomeDataProcessingProgram", account_id: "222222222222", region: "us-west-2" }
/// ...
/// ```
///
/// ## Commands
///
/// You can use `cargo run --` to build and pass commands to the CLI.
///
/// ```bash
/// # run retry counts, replace START_TIME in retry-counts graph to start 6 months ago
/// cargo run -- images --period 3600 --pattern ItemDPP -s 4320H ./resources/traffic.json ../accounts.toml
///
/// # omit the pattern to run this command for all accounts
/// cargo run -- images --period 3600  -s 7200H ./resources/traffic.json ../accounts.toml
/// ```
#[tokio::main]
async fn main() -> Result<(), Error> {
    tracing_subscriber::fmt::init();

    let matches = Command::new("dev")
        .subcommand(
            Command::new("alarms")
                .about("describe alarms for all accounts")
                .arg(
                    Arg::new("pattern")
                        .long("pattern")
                        .takes_value(true)
                        .short('f'),
                )
                .arg(
                    Arg::new("config-path")
                        .required(true)
                        .help("the path to the TOML config file with accounts"),
                ),
        )
        .subcommand(
            Command::new("images")
                .about("download metric widget images from CloudWatch")
                .arg(
                    Arg::new("region")
                        .help("AWS region (e.g. us-east-1, eu-west-1)")
                        .long("region")
                        .short('r')
                        .takes_value(true),
                )
                .arg(
                    Arg::new("start-time")
                        .short('s')
                        .default_value("4320H")
                        .long("start-time")
                        .alias("start")
                        .takes_value(true),
                )
                .arg(
                    Arg::new("end-time")
                        .short('e')
                        .default_value("0H")
                        .alias("end")
                        .takes_value(true),
                )
                .arg(
                    Arg::new("period")
                        .short('p')
                        .default_value("3600")
                        .long("period")
                        .takes_value(true),
                )
                .arg(
                    Arg::new("title")
                        .long("title")
                        .help("title to identify the image downloaded")
                        .default_value("metric")
                        .takes_value(true),
                )
                .arg(Arg::new("template-path").required(true))
                .arg(
                    Arg::new("config-path")
                        .required(true)
                        .help("the path to the TOML config file with accounts"),
                )
                .arg(
                    Arg::new("pattern")
                        .long("pattern")
                        .takes_value(true)
                        .short('f'),
                )
                .arg(
                    Arg::new("output-path")
                        .required(false)
                        .long("output-path")
                        .short('o'),
                ),
        )
        .subcommand(
            Command::new("config")
                .about("validate and display the config file for your accounts")
                .arg(Arg::new("config-path").required(true))
                .arg(
                    Arg::new("pattern")
                        .long("pattern")
                        .takes_value(true)
                        .short('f'),
                ),
        )
        .subcommand(Command::new("show").about("show metrics for an account"))
        .get_matches();

    match matches.subcommand() {
        Some(("images", images)) => {
            let start = images.value_of("start-time").unwrap();
            let end = images.value_of("end-time").unwrap();
            let template_path = images.value_of("template-path").unwrap();
            let period = images.value_of("period").unwrap();
            let title = images.value_of("title").unwrap();
            let config_path = images.value_of("config-path").unwrap();
            let pattern = images.value_of("pattern");
            let accounts = get_accounts(config_path, true);
            let accounts = filter_accounts(pattern, accounts);

            for acc in accounts {
                let props = GetWidgetProps {
                    title: String::from(title),
                    region: Some(acc.region),
                    app_name: acc.namespace,
                    role_arn: acc.role_arn,
                    template_path: PathBuf::from(template_path),
                    start: String::from(start),
                    end: String::from(end),
                    period: String::from(period),
                    verbose: true,
                };
                match cloudwatch_image_download(props).await {
                    Ok(_) => println!("successful query"),
                    Err(e) => println!("cloudwatch download error: {:?}", e),
                };
            }
        }
        Some(("show", show_matches)) => {
            println!("show: {:?}", show_matches);

            let client = get_cw_client("us-west-2", true).await;
            let res = show_metrics(&client).await;
            if res.is_err() {
                println!("encountered error getting metrics: {:?}", res.err());
            }
        }
        Some(("alarms", alarm_matches)) => {
            let pattern = alarm_matches.value_of("pattern");
            let config_path = alarm_matches.value_of("config-path").unwrap();
            let accounts = get_accounts(config_path, true);
            let accounts = filter_accounts(pattern, accounts);
            let mut all_metrics: Vec<MetricAlarmDetails> = vec![];
            for acc in accounts {
                println!("account: {:?}", acc);
                let props = DescribeAlarmsProps {
                    region: Some(acc.region),
                    role_arn: acc.role_arn,
                    verbose: true,
                };
                match cloudwatch_describe_alarms(props).await {
                    Ok(res) => {
                        println!("successful query");
                        for item in res {
                            let comparison = match item.comparison_operator().unwrap() {
                                ComparisonOperator::GreaterThanOrEqualToThreshold => {
                                    "GreaterThanOrEqualToThreshold"
                                }
                                ComparisonOperator::GreaterThanThreshold => "GreaterThanThreshold",
                                ComparisonOperator::LessThanThreshold => "LessThanThreshold",
                                ComparisonOperator::LessThanOrEqualToThreshold => {
                                    "LessThanOrEqualToThreshold"
                                }
                                _ => "Unknown",
                            };
                            let statistic = match item.statistic() {
                                Some(some) => match some {
                                    Statistic::Average => "Average",
                                    Statistic::Maximum => "Maximum",
                                    Statistic::Minimum => "Minimum",
                                    Statistic::SampleCount => "SampleCount",
                                    Statistic::Sum => "Sum",
                                    _ => "Unknown",
                                },
                                None => "",
                            };
                            all_metrics.push(MetricAlarmDetails {
                                program_name: acc.namespace.clone(),
                                alarm_name: String::from(item.alarm_name().unwrap_or_default()),
                                alarm_arn: String::from(item.alarm_arn().unwrap_or_default()),
                                alarm_description: String::from(
                                    item.alarm_description().unwrap_or_default(),
                                ),
                                dimensions: item
                                    .dimensions()
                                    .unwrap()
                                    .iter()
                                    .map(|i| String::from(i.name().unwrap()))
                                    .collect(),
                                actions_enabled: item.actions_enabled().unwrap_or_default(),
                                period: item.period().unwrap_or_default(),
                                threshold: item.threshold().unwrap_or_default(),
                                comparison_operator: String::from(comparison),
                                treat_missing_data: String::from(
                                    item.treat_missing_data().unwrap_or_default(),
                                ),
                                statistic: String::from(statistic),
                            });
                        }
                    }
                    Err(e) => println!("failed describe alarms error: {:?}", e),
                }
            }
            let path = Path::new("describe-alarms").with_extension("json");
            let as_str = serde_json::to_string(&all_metrics).unwrap();
            let res = fs::write(path, as_str).await;
            match res {
                Ok(()) => {
                    println!("saved metrics");
                }
                Err(e) => {
                    println!("error writing to file: {:?}", e);
                }
            }
        }
        Some(("config", config)) => {
            let config_path = config.value_of("config-path").unwrap();
            let pattern = config.value_of("pattern");
            let accounts = get_accounts(config_path, true);
            let _filtered = filter_accounts(pattern, accounts);
        }
        _ => unreachable!(),
    };

    Ok(())
}

fn filter_accounts(pattern: Option<&str>, accounts: Option<AccountsConfig>) -> Vec<AccountConfig> {
    if let Some(pat) = pattern {
        let pat = String::from(pat);
        let filtered: Vec<AccountConfig> = accounts
            .unwrap()
            .account
            .into_iter()
            .filter(|x| x.namespace.contains(&pat))
            .collect();
        println!("Filtered accounts:");
        for acc in &filtered {
            println!("{:?}", &acc);
        }
        filtered
    } else {
        accounts.expect("expected accounts to filter").account
    }
}

async fn get_cw_client(region: &str, verbose: bool) -> cloudwatchClient {
    let static_region = aws_regions::convert_to_name(region);

    if verbose {
        println!();
        println!("CloudWatch client version: {}", PKG_VERSION);
        println!("Region:                    {}", static_region);
        println!();
    }

    let shared_config = aws_config::from_env().region(static_region).load().await;

    if verbose {
        println!();
        println!("SdkConfig: {:?}", shared_config);
        println!();
    }

    cloudwatchClient::new(&shared_config)
}

async fn get_sts_client(region: &str, verbose: bool) -> stsClient {
    let static_region = aws_regions::convert_to_name(region);

    if verbose {
        println!();
        println!("CloudWatch client version: {}", PKG_VERSION);
        println!("Region:                    {}", static_region);
        println!();
    }

    let shared_config = aws_config::from_env().region(static_region).load().await;
    stsClient::new(&shared_config)
}

async fn get_cw_client_with_role(
    region: &str,
    role_arn: &str,
    sts_client: &stsClient,
    verbose: bool,
) -> cloudwatchClient {
    let static_region = aws_regions::convert_to_name(region);

    if verbose {
        println!();
        println!("Client versions: {}", PKG_VERSION);
        println!("Region:                    {}", static_region);
        println!("Role Arn:                  {}", role_arn);
        println!();
    }

    let assumed_role = sts_client
        .assume_role()
        .role_arn(role_arn)
        .role_session_name("dev-cli")
        .send()
        .await
        .unwrap();

    let creds = aws_types::Credentials::new(
        assumed_role.credentials().unwrap().access_key_id().unwrap(),
        assumed_role
            .credentials()
            .unwrap()
            .secret_access_key()
            .unwrap(),
        Some(
            assumed_role
                .credentials()
                .unwrap()
                .session_token()
                .unwrap()
                .into(),
        ),
        Some(std::time::UNIX_EPOCH + Duration::from_secs(1800)),
        "dev-cli-metrics-observer",
    );

    let shared_config = aws_config::from_env()
        .region(static_region) // specify the region again for this specific account, need to make sure this matches the account's infrastructure region
        .credentials_provider(creds)
        .load()
        .await;
    cloudwatchClient::new(&shared_config)
}

async fn cloudwatch_describe_alarms(opts: DescribeAlarmsProps) -> Result<Vec<MetricAlarm>, Error> {
    let DescribeAlarmsProps {
        region,
        role_arn,
        verbose,
    } = opts;
    let replaced_region = region.clone().unwrap_or_else(|| String::from("us-west-2"));
    let sts_client = get_sts_client(&replaced_region.as_str(), verbose).await;
    let client = get_cw_client_with_role(
        &replaced_region.as_str(),
        role_arn.as_str(),
        &sts_client,
        verbose,
    )
    .await;
    describe_alarms(&client).await
}

async fn cloudwatch_image_download(opts: GetWidgetProps) -> Result<(), Error> {
    let GetWidgetProps {
        app_name: namespace,
        end,
        period,
        region,
        role_arn,
        start,
        template_path: filepath,
        title,
        verbose,
    } = opts;

    let replaced_region = region.clone().unwrap_or_else(|| String::from("us-west-2"));

    let sts_client = get_sts_client(&replaced_region.as_str(), verbose).await;
    let client = get_cw_client_with_role(
        &replaced_region.as_str(),
        role_arn.as_str(),
        &sts_client,
        verbose,
    )
    .await;
    if let Some(metrics) = get_metrics_json(
        &filepath,
        &replaced_region,
        &namespace,
        &start,
        &end,
        &period,
        verbose,
    ) {
        let saved_image_name = format!(
            "{}-{}-{}-{}-{}",
            &namespace,
            &title,
            &replaced_region,
            &start,
            std::time::SystemTime::now()
                .duration_since(std::time::SystemTime::UNIX_EPOCH)
                .unwrap()
                .as_secs()
        );
        get_metric_image(&client, metrics.as_ref(), &saved_image_name).await
    } else {
        panic!("unable to parse metrics json")
    }
}

fn get_accounts(filepath: &str, verbose: bool) -> Option<AccountsConfig> {
    let config_file = std::fs::read_to_string(filepath);
    if let Ok(contents) = config_file {
        let accounts_config: AccountsConfig =
            toml::from_str(&contents).expect("unable to parse as toml");
        if verbose {
            for acc in &accounts_config.account {
                println!("{:?}", acc)
            }
        }
        Some(accounts_config)
    } else {
        None
    }
}

fn get_metrics_json(
    filepath: &PathBuf,
    region: &str,
    namespace: &str,
    start: &str,
    end: &str,
    period: &str,
    verbose: bool,
) -> Option<String> {
    let template_file = std::fs::read_to_string(filepath);
    if let Ok(contents) = template_file {
        let mut template_params = HashMap::<&str, &str>::new();

        // TODO: make this configurable
        template_params.insert("{{NAMESPACE}}", namespace);
        template_params.insert("{{REGION}}", region);
        // format: 4320H
        template_params.insert("{{PERIOD_START}}", start);
        template_params.insert("{{PERIOD_END}}", end);
        template_params.insert("{{PERIOD}}", period);

        let mut replaced = contents;
        template_params
            .iter()
            .for_each(|(k, v)| replaced = replaced.replace(k, v));

        if verbose {
            println!("templated:\n{}", &replaced);
        }

        Some(replaced)
    } else {
        None
    }
}

// List metrics.
async fn show_metrics(
    client: &aws_sdk_cloudwatch::Client,
) -> Result<(), aws_sdk_cloudwatch::Error> {
    let rsp = client.list_metrics().send().await?;
    let metrics = rsp.metrics().unwrap_or_default();

    let num_metrics = metrics.len();

    for metric in metrics {
        println!("Namespace: {}", metric.namespace().unwrap_or_default());
        println!("Name:      {}", metric.metric_name().unwrap_or_default());
        println!("Dimensions:");

        if let Some(dimension) = metric.dimensions.as_ref() {
            for d in dimension {
                println!("  Name:  {}", d.name().unwrap_or_default());
                println!("  Value: {}", d.value().unwrap_or_default());
                println!();
            }
        }

        println!();
    }

    println!("Found {} metrics.", num_metrics);

    Ok(())
}

async fn describe_alarms(
    client: &aws_sdk_cloudwatch::Client,
) -> Result<Vec<MetricAlarm>, aws_sdk_cloudwatch::Error> {
    println!("describing alarms");
    let request = client.describe_alarms();
    let resp = request.send().await?;
    let alarms = resp.metric_alarms().unwrap();
    let vec: Vec<MetricAlarm> = alarms.to_vec();
    Ok(vec)
}

/// Calls AWS CloudWatch GetMetricImage API and downloads locally
/// API Reference: [GetMetricWidgetImage](https://docs.aws.amazon.com/AmazonCloudWatch/latest/APIReference/API_GetMetricWidgetImage.html)
async fn get_metric_image(
    client: &aws_sdk_cloudwatch::Client,
    metric_json: &str,
    saved_image_name: &str,
) -> Result<(), aws_sdk_cloudwatch::Error> {
    println!("getting metric image");

    let request = client
        .get_metric_widget_image()
        .output_format("png")
        .set_metric_widget(Some(String::from(metric_json)));
    let resp = request.send().await?;

    if let Some(blob) = resp.metric_widget_image {
        let path = Path::new(saved_image_name).with_extension("png");

        // convert to base64 encoded byte vector
        let base64_encoded = blob.into_inner();

        // wait to finish saving file
        let res = fs::write(path, base64_encoded).await;
        match res {
            Ok(()) => {
                println!("saved metric image");
            }
            Err(e) => {
                println!("error writing to file: {:?}", e);
            }
        }
    } else {
        println!("error getting metric image");
    }
    Ok(())
}
