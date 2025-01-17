mod codec;
pub mod error;

use encoding::DecoderTrap;
use encoding::{all::GB18030, all::GBK, Encoding};
use futures::stream::StreamExt;
use regex::Regex;
use tokio::{
    io::AsyncWriteExt,
    net::TcpStream,
    time::{self, Duration},
};
use tokio_util::codec::FramedRead;

use crate::codec::{Item, TelnetCodec};
use crate::error::TelnetError;

#[derive(Debug)]
pub struct TelnetBuilder {
    prompts: Vec<String>,
    page_separator: String,
    connect_timeout: Duration,
    timeout: Duration,
}

impl Default for TelnetBuilder {
    fn default() -> Self {
        Self {
            prompts: vec![r"\w+#\s*".into(), r"\w+\$\s*".into()],
            page_separator: r"--More--".into(),
            connect_timeout: Duration::from_secs(5),
            timeout: Duration::from_secs(5),
        }
    }
}

impl TelnetBuilder {
    /// Set the telnet server prompt, as many characters as possible.(`~` or `#` is not good. May misjudge).
    pub fn prompt<T: ToString>(mut self, prompt: T) -> TelnetBuilder {
        self.prompts = vec![prompt.to_string()];
        self
    }

    /// Set the telnet server prompts, as many characters as possible.(`~` or `#` is not good. May misjudge).
    /// If `prompts` is set, `prompt` will be overwritten.
    pub fn prompts<T: ToString>(mut self, prompts: &[T]) -> TelnetBuilder {
        self.prompts = prompts.iter().map(|p| p.to_string()).collect();
        self
    }

    pub fn page_separator<T: ToString>(mut self, page_separator: T) -> TelnetBuilder {
        self.page_separator = page_separator.to_string();
        self
    }

    /// Set the timeout for `TcpStream` connect remote addr.
    pub fn connect_timeout(mut self, connect_timeout: Duration) -> TelnetBuilder {
        self.connect_timeout = connect_timeout;
        self
    }

    /// Set the timeout for the operation.
    pub fn timeout(mut self, timeout: Duration) -> TelnetBuilder {
        self.timeout = timeout;
        self
    }

    /// Establish a connection with the remote telnetd.
    pub async fn connect(self, addr: &str) -> Result<Telnet, TelnetError> {
        let mut prompts = vec![];
        for s in self.prompts {
            prompts.push(s.parse()?);
        }

        match time::timeout(self.connect_timeout, TcpStream::connect(addr)).await {
            Ok(res) => Ok(Telnet {
                stream: res?,
                timeout: self.timeout,
                prompts,
                page_separator: self.page_separator.parse()?,
                buffer: String::with_capacity(8192),
            }),
            Err(_) => Err(TelnetError::Timeout(format!(
                "Connect remote addr({})",
                addr
            ))),
        }
    }
}

pub struct Telnet {
    timeout: Duration,
    stream: TcpStream,
    prompts: Vec<Regex>,
    page_separator: Regex,
    buffer: String,
}

impl Telnet {
    /// Create a `TelnetBuilder`
    pub fn builder() -> TelnetBuilder {
        TelnetBuilder::default()
    }
    // Format the end of the string as a `\n`
    fn format_enter_str(s: &str) -> String {
        if !s.ends_with('\n') {
            format!("{}\n", s)
        } else {
            s.to_string()
        }
    }

    pub async fn send(&mut self, cmd: &str) -> Result<(), TelnetError> {
        log::trace!("Send '{}'", cmd);

        let command = Telnet::format_enter_str(cmd);

        let (_, mut write) = self.stream.split();
        match time::timeout(self.timeout, write.write(command.as_bytes())).await {
            Ok(res) => res?,
            Err(_) => return Err(TelnetError::Timeout("write cmd".to_string())),
        };

        Ok(())
    }

    pub async fn wait_with(&mut self, prompt: &str) -> Result<String, TelnetError> {
        log::trace!("Wait with `{}`", prompt);

        let prompt: Regex = prompt.parse()?;
        let (read, mut write) = self.stream.split();
        let mut telnet = FramedRead::new(read, TelnetCodec::default());
        let mut page_cache = String::new();

        'outer: loop {
            match time::timeout(self.timeout, telnet.next()).await {
                Ok(res) => match res {
                    Some(item) => {
                        if let Item::Line(line) = item? {
                            let line = decode(&line)?;

                            log::trace!("Recv '{}', raw: {:?}", line, line.as_bytes());

                            // self.buffer.push_str(&line);

                            // if self.page_separator.is_match(&line) {
                            //     // Print next page
                            //     write.write(" ".as_bytes()).await?;
                            // }

                            // if prompt.is_match(&line) {
                            //     break 'outer;
                            // }
                            page_cache.push_str(&line);

                            if self.page_separator.is_match(&page_cache) {
                                // Print next page
                                write.write(" ".as_bytes()).await?;
                                self.buffer.push_str(&page_cache);

                                if prompt.is_match(&page_cache) {
                                    break 'outer;
                                }

                                page_cache.clear();
                            }

                            if prompt.is_match(&page_cache) {
                                self.buffer.push_str(&page_cache);
                                break 'outer;
                            }
                        }
                    }
                    None => return Err(TelnetError::NoMoreData),
                },
                Err(_) => return Err(TelnetError::Timeout("read next framed".to_string())),
            }
        }

        // Remove page_separator
        let mut res = self
            .page_separator
            .replace_all(&self.buffer, "")
            .to_string();

        // Remove prompt
        res = prompt.replace_all(&res, "").to_string();

        // Trim result
        res = res.trim().to_string();

        // Clear buffer
        self.buffer.clear();

        Ok(res)
    }

    pub async fn wait(&mut self) -> Result<String, TelnetError> {
        log::trace!("Wait");

        let (read, mut write) = self.stream.split();
        let mut telnet = FramedRead::new(read, TelnetCodec::default());

        'outer: loop {
            match time::timeout(self.timeout, telnet.next()).await {
                Ok(res) => match res {
                    Some(item) => {
                        if let Item::Line(line) = item? {
                            let line = decode(&line)?;

                            log::trace!("Recv '{}', raw: {:?}", line, line.as_bytes());

                            self.buffer.push_str(&line);

                            if self.page_separator.is_match(&line) {
                                // Print next page
                                write.write(" \n".as_bytes()).await?;
                            }

                            for prompt in &self.prompts {
                                if prompt.is_match(&line) {
                                    break 'outer;
                                }
                            }
                        }
                    }
                    None => return Err(TelnetError::NoMoreData),
                },
                Err(_) => return Err(TelnetError::Timeout("read next framed".to_string())),
            }
        }

        // Remove page_separator
        let mut res = self
            .page_separator
            .replace_all(&self.buffer, "")
            .to_string();

        // Remove prompt
        for prompt in &self.prompts {
            res = prompt.replace_all(&res, "").to_string();
        }

        // Trim result
        res = res.trim().to_string();

        // Clear buffer
        self.buffer.clear();

        Ok(res)
    }

    pub async fn exec(&mut self, cmd: &str) -> Result<String, TelnetError> {
        self.send(cmd).await?;
        let out = self.wait().await?;

        if let Some(out) = out.strip_prefix(cmd) {
            Ok(out.trim().into())
        } else {
            Ok(out)
        }
    }
}

fn decode(line: &[u8]) -> Result<String, TelnetError> {
    match String::from_utf8(line.to_vec()) {
        Ok(result) => Ok(result),
        Err(e) => {
            if let Ok(result) = GBK.decode(line, DecoderTrap::Strict) {
                return Ok(result);
            }

            if let Ok(result) = GB18030.decode(line, DecoderTrap::Strict) {
                return Ok(result);
            }
            Err(TelnetError::ParseError(e))
        }
    }
}
