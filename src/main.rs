use dotenvy::dotenv;
use futures_util::future::{select, Either};
use grammers_client::session::Session;
use grammers_client::{Client, Config, InitParams, ReconnectionPolicy, Update};
use log::{error, info, LevelFilter};
use serde::{Deserialize, Serialize};
use std::env;
use std::fs::File;
use std::io::{stdin, stdout, BufReader, Write};
use std::ops::ControlFlow;
use std::path::Path;
use std::pin::pin;
use std::sync::Arc;
use std::time::Duration;
use tokio::{runtime, task};

const MAX_RETRIES: i32 = 3;
const SESSION_FILE: &str = "first.session";

#[derive(Serialize, Deserialize, Debug, Clone)]
struct AppConfig {
    sources: Vec<i64>,
    targets: Vec<i64>,
    verbose: bool,
}

struct MyPolicy;

impl ReconnectionPolicy for MyPolicy {
    fn should_retry(&self, attempts: usize) -> ControlFlow<(), Duration> {
        if attempts > 10 {
            error!("Too many reconnection attempts, giving up");
            return ControlFlow::Break(());
        }

        let duration = std::cmp::min(u64::pow(2, attempts as u32), 60);
        info!(
            "Reconnecting in {} seconds (attempt {})",
            duration,
            attempts + 1
        );
        ControlFlow::Continue(Duration::from_secs(duration))
    }
}

fn is_channel_allowed(channels: &[i64], channel_id: i64) -> bool {
    channels.contains(&channel_id)
}

async fn handle_update(
    config: Arc<AppConfig>,
    client: Client,
    update: Update,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    match update {
        Update::NewMessage(message) if !message.outgoing() => {
            let chat = message.chat();
            let chat_id = chat.id();

            if config.verbose {
                let chat_name = chat.name();
                let message_text = message.text();
                info!(
                    "\nChat: {}\nFrom: {}\n Message: {}\n",
                    chat_id, chat_name, message_text
                );
            }

            if is_channel_allowed(&config.sources, chat_id) {
                for target in &config.targets {
                    let mut dialogs = client.iter_dialogs();
                    while let Some(dialog) = dialogs.next().await? {
                        let dest_chat = dialog.chat();
                        if dest_chat.id() == *target {
                            let mut try_count = 0;
                            while try_count < MAX_RETRIES {
                                match client
                                    .forward_messages(
                                        dest_chat.pack(),
                                        &[message.id()],
                                        message.chat().pack(),
                                    )
                                    .await
                                {
                                    Ok(_) => {
                                        info!("Message forwarded from {} to {}", chat_id, target);
                                        break;
                                    }
                                    Err(e) => {
                                        error!("Error forwarding message: {}", e);
                                        try_count += 1;
                                    }
                                }
                            }
                            break;
                        }
                    }
                }
            }
        }
        _ => {}
    }
    Ok(())
}

async fn periodic_health_check(client: &Client) {
    info!("Starting periodic health check...");
    loop {
        tokio::time::sleep(Duration::from_secs(300)).await; // Every 5 minutes

        match client.get_me().await {
            Ok(_) => info!("Health check: Connection OK"),
            Err(e) => {
                error!("Health check failed: {}", e);
            }
        }
    }
}

async fn async_main(
    config: Arc<AppConfig>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let api_id: i32 = env::var("TG_ID")
        .expect("TG_ID is missing")
        .parse()
        .expect("TG ID format is wrong, please use integer");
    let api_hash = env::var("TG_HASH").expect("TG_HASH is missing").to_string();
    let phone = env::var("PHONE").expect("PHONE is missing").to_string();
    // let token = env::args().nth(1).expect("token missing");

    info!("Connecting to Telegram...");
    let client = Client::connect(Config {
        session: Session::load_file_or_create(SESSION_FILE)?,
        api_id,
        api_hash,
        params: InitParams {
            reconnection_policy: &MyPolicy,
            ..Default::default()
        },
    })
    .await?;

    info!("Connected to Telegram!");

    client.session().save_to_file(SESSION_FILE)?;

    if !client
        .is_authorized()
        .await
        .expect("Failed to get authorization status")
    {
        info!("Not authorized, signing in...");

        // Start the sign-in process
        let token = client
            .request_login_code(&phone)
            .await
            .expect("Failed to request login code");

        let code = prompt("Please enter the code you received: ");
        client
            .sign_in(&token, &code)
            .await
            .expect("Failed to sign in with the code");

        // Check if we need 2FA
        // match client
        //     .is_authorized()
        //     .await
        //     .expect("Failed to get authorization status")
        // {
        //     true => info!("Successfully signed in!"),
        //     false => {
        //         let password = prompt("Please enter your 2FA password: ");
        //         client
        //             .check_password(password.as_bytes())
        //             .await
        //             .expect("Failed to sign in with 2FA");
        //         info!("Successfully signed in with 2FA!");
        //     }
        // }
    }

    info!(
        "Connected as {}!",
        client.get_me().await?.username().unwrap()
    );
    info!("Waiting for messages...");
    let client1 = client.clone();
    tokio::spawn(async move { periodic_health_check(&client1).await });

    // This code uses `select` on Ctrl+C to gracefully stop the client and have a chance to
    // save the session. You could have fancier logic to save the session if you wanted to
    // (or even save it on every update). Or you could also ignore Ctrl+C and just use
    // `let update = client.next_update().await?`.
    //
    // Using `tokio::select!` would be a lot cleaner but add a heavy dependency,
    // so a manual `select` is used instead by pinning async blocks by hand.
    loop {
        let exit = pin!(async { tokio::signal::ctrl_c().await });
        let upd = pin!(async { client.next_update().await });

        let update = match select(exit, upd).await {
            Either::Left(_) => break,
            Either::Right((u, _)) => match u {
                Ok(update) => update,
                Err(e) => {
                    error!("Error getting update: {}", e);
                    tokio::time::sleep(Duration::from_secs(5)).await;
                    continue;
                }
            },
        };

        let handle = client.clone();
        let config = Arc::clone(&config);
        task::spawn(async move {
            if let Err(e) = handle_update(config, handle, update).await {
                error!("Error handling updates!: {e}");
            }
        });
    }

    info!("Saving session file and exiting...");
    client.session().save_to_file(SESSION_FILE)?;
    Ok(())
}

fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    pretty_env_logger::formatted_builder()
        .filter_level(LevelFilter::Info)
        .filter_module("grammers_session::message_box", LevelFilter::Warn)
        .init();

    dotenv().ok();

    let config: AppConfig = load_config("./config.json").expect("Failed to load config");

    runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap()
        .block_on(async_main(Arc::new(config)))
}

fn prompt(message: &str) -> String {
    print!("{}", message);
    stdout().flush().unwrap();
    let mut input = String::new();
    stdin().read_line(&mut input).expect("Failed to read input");
    input.trim().to_string()
}

fn load_config<P: AsRef<Path>>(path: P) -> Result<AppConfig, std::io::Error> {
    let file = File::open(path)?;
    let reader = BufReader::new(file);
    let config: AppConfig = serde_json::from_reader(reader)?;
    Ok(config)
}
