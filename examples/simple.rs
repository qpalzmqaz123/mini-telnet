use std::time::Duration;

use mini_telnet::Telnet;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    env_logger::init();

    let mut telnet = Telnet::builder()
        .prompt(r"Switch#\s+")
        .connect_timeout(Duration::from_secs(2))
        .timeout(Duration::from_secs(2))
        .page_separator(r"\[7m--More--\[m")
        .connect("192.168.10.254:23")
        .await?;

    let out = telnet.wait_with(r"Switch#\s+").await?;
    println!("out1: '{}'", out);

    telnet.send("show version").await?;
    let out = telnet.wait_with(r"Switch#\s+").await?;
    println!("out2: '{}'", out);

    telnet.send("show boot").await?;
    let out = telnet.wait_with(r"Switch#\s+").await?;
    println!("out3: '{}'", out);

    let out = telnet.exec("show vlan").await?;
    println!("out4: '{}'", out);

    telnet.send("start shell").await?;
    telnet.wait_with(r"Password:\s+").await?;
    telnet.send("!@#").await?;
    telnet.wait_with(r"\[root@.*?\]\$\s+").await?;
    telnet.send("ip a").await?;
    let out = telnet.wait_with(r"\[root@.*?\]\$\s+").await?;
    println!("out5: '{}'", out);

    Ok(())
}
