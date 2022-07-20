use std::time::Duration;

use mini_telnet::Telnet;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    env_logger::init();

    let mut telnet = Telnet::builder()
        .prompt("Switch#")
        .connect_timeout(Duration::from_secs(2))
        .timeout(Duration::from_secs(2))
        .page_separator(r"\[7m--More--\[m")
        .connect("192.168.10.254:23")
        .await?;

    let out = telnet.wait().await?;
    println!("out1: '{}'", out);

    telnet.send("show version").await?;
    let out = telnet.wait().await?;
    println!("out2: '{}'", out);

    telnet.send("show boot").await?;
    let out = telnet.wait().await?;
    println!("out3: '{}'", out);

    let out = telnet.exec("show vlan").await?;
    println!("out4: '{}'", out);

    Ok(())
}
