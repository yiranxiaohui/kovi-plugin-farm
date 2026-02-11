use std::collections::VecDeque;
use std::fs;
use std::fs::File;
use std::io::{Cursor, Read, Write};
use std::path::PathBuf;
use std::process::Stdio;
use std::sync::{Arc, LazyLock};
use std::time::Duration;
use kovi::{Message, MsgEvent, PluginBuilder as plugin, PluginBuilder};
use kovi::log::{debug, error, info};
use kovi::tokio::time::{interval, timeout};
use reqwest::{Client};
use reqwest::header::HeaderMap;
use serde::{Deserialize, Serialize};
use tokio::process::{Child, Command};
use dashmap::DashMap;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::sync::Mutex;

static STATUS: LazyLock<DashMap<String, Process>> = LazyLock::new(|| DashMap::new());

struct Process {
    child: Child,
    output: Arc<Mutex<VecDeque<String>>>,
}

#[kovi::plugin]
async fn main() {
    let bot = PluginBuilder::get_runtime_bot();
    let data_path = bot.get_data_path();
    let path = check_exists(data_path).await;
    plugin::on_msg(move |event| {
        let bot = bot.clone();
        let path = path.clone();
        async move {
            let text = event.borrow_text().unwrap_or("");
            if text.starts_with("登录农场") {
                let data = get_login_url(event.clone(), path).await;
                let msg = Message::new()
                    .add_text(data.url);
                event.reply(msg);
            } else if text.starts_with("农场状态") {
                if let Some(status) = STATUS.get(&event.get_sender_nickname()) {
                    let output = get_output(event.get_sender_nickname().clone()).await;
                    let mut text = format!("农场状态：\n");
                    for i in output {
                        text.push_str(format!("{}\n", i).as_str());
                    }
                    let msg = Message::new()
                        .add_text(text);
                    event.reply(msg);
                } else {
                    let msg = Message::new()
                        .add_text("您还未登录，请先登录后操作");
                    event.reply(msg);
                }
            } else if text.starts_with("获取QQ名") {
                let msg = Message::new()
                    .add_text(format!("{}", event.get_sender_nickname()));
                event.reply(msg);
            }
        }
    });
}

async fn get_output(nickname: String) -> VecDeque<String> {
    if let Some(entry) = STATUS.get(&nickname) {
        entry.output.lock().await.clone()
    } else {
        VecDeque::new()
    }
}

async fn check_exists(mut data_path: PathBuf) -> PathBuf {
    data_path.push("qq-farm-bot");
    if data_path.exists() && data_path.is_dir() {
        info!("目录存在，无需下载");
    } else {
        info!("目录不存在，触发下载请求");
        download(data_path.clone()).await.unwrap();
    }

    // 执行 npm install
    info!("正在执行 npm install...");

    let status = if cfg!(target_os = "windows") {
        Command::new("cmd")
            .args(["/C", "npm", "install"])
            .current_dir(&data_path)
            .status()
    } else {
        Command::new("npm")
            .arg("install")
            .current_dir(&data_path)
            .status()
    };

    match status.await {
        Ok(s) if s.success() => info!("npm install 完成"),
        Ok(s) => error!("npm install 失败，退出码: {:?}", s.code()),
        Err(e) => error!("npm install 执行失败: {}", e),
    }

    data_path
}

async fn download(path: PathBuf) -> Result<String, Box<dyn std::error::Error>> {
    let owner = "ryunnet";
    let repo = "qq-farm-bot";
    let branch = "main";

    let url = format!(
        "https://github.com/{owner}/{repo}/archive/refs/heads/{branch}.zip"
    );

    info!("正在下载 {owner}/{repo}...");
    let response = reqwest::get(&url).await?;

    if !response.status().is_success() {
        return Err(format!("下载失败: {}", response.status()).into());
    }

    let bytes = response.bytes().await?;

    // 解压
    let cursor = Cursor::new(bytes);
    let mut archive = zip::ZipArchive::new(cursor)?;

    let output_dir = path;

    for i in 0..archive.len() {
        let mut entry = archive.by_index(i)?;
        let name = entry.name().to_string();

        // 去掉第一层目录（如"qq-farm-bot-main/"）
        let stripped = match name.find('/') {
            Some(pos) => &name[pos + 1..],
            None => continue,
        };

        //跳过空路径（即根目录本身）
        if stripped.is_empty() {
            continue;
        }

        let out_path = output_dir.join(stripped);

        if entry.is_dir() {
            fs::create_dir_all(&out_path)?;
        } else {
            if let Some(parent) = out_path.parent() {
                fs::create_dir_all(parent)?;
            }
            let mut buf = Vec::new();
            entry.read_to_end(&mut buf)?;
            let mut file = File::create(&out_path)?;
            file.write_all(&buf)?;
        }
    }

    info!("解压完成到 {:?}", output_dir);

    return Ok(String::new());
}

async fn get_login_url(event: Arc<MsgEvent>, path: PathBuf) -> Data {
    let client = Client::builder().build().unwrap();
    let res = client.get("https://q.qq.com/ide/devtoolAuth/GetLoginCode").headers(get_headers()).send().await.unwrap();
    let text = res.text().await.unwrap();
    let res: Login = serde_json::from_str(&text).unwrap();
    info!("{:#?}", res);

    let data = Data {
        code: res.data.code.clone(),
        url: format!("https://h5.qzone.qq.com/qqq/code/{}?_proxy=1&from=ide", res.data.code.clone())
    };

    let code = data.code.clone();
    tokio::spawn(async move {
        let result = timeout(Duration::from_secs(300), async {
            let mut ticker = interval(Duration::from_secs(2));
            loop {
                ticker.tick().await;
                let data = check_login_status(code.clone()).await;
                if let Some(ok) = data.data.ok {
                    if ok == 1 {
                        info!("data = {:?}", data);
                        event.reply(format!("正在为[ {} ]启动脚本！", event.get_sender_nickname()));
                        if STATUS.contains_key(&event.get_sender_nickname()) {
                            if let Some(mut child) = STATUS.get_mut(&event.get_sender_nickname()) {
                                child.child.kill().await.ok();
                            }
                            STATUS.remove(&event.get_sender_nickname());
                        }
                        let code = get_auth_code(data.data.ticket.unwrap()).await;
                        start(code, path.clone(), event.get_sender_nickname()).await;
                        break;
                    }
                }
            }
        }).await;
    });

    data
}

async fn start(code: String, path: PathBuf, nickname: String) {
    let mut child = if cfg!(target_os = "windows") {
        Command::new("cmd")
            .args(["/C", "node", "client.js", "--code", code.as_str()])
            .stdout(Stdio::piped())
            .current_dir(&path)
            .spawn().unwrap()
    } else {
        Command::new("node")
            .args(["client.js", "--code", code.as_str()])
            .stdout(Stdio::piped())
            .current_dir(&path)
            .spawn().unwrap()
    };
    let stdout = child.stdout.take().unwrap();
    let output = Arc::new(Mutex::new(VecDeque::new()));

    let process = Process {
        child,
        output: Arc::clone(&output),
    };
    STATUS.insert(nickname.clone(), process);

    // 异步收集输出
    let task_name = nickname.clone();
    tokio::spawn(async move {
        let mut reader = BufReader::new(stdout).lines();
        while let Some(line) = reader.next_line().await.unwrap() {
            println!("[{}] {}", task_name, line);
            let mut output = output.lock().await;
            if output.len() >= 10 {
                output.pop_front();
            }
            output.push_back(line);
        }
    });
}

async fn check_login_status(code: String) -> Login {
    let client = Client::builder().build().unwrap();
    let url = format!("https://q.qq.com/ide/devtoolAuth/syncScanSateGetTicket?code={code}");
    let res = client.get(url).headers(get_headers()).send().await.unwrap();
    let text = res.text().await.unwrap();

    let data: Login = serde_json::from_str(text.as_str()).unwrap();

    data
}

#[derive(Deserialize, Debug)]
struct Data {
    code: String,
    url: String
}

#[derive(Deserialize, Debug)]
struct Login {
    code: i16,
    data: LoginData,
    message: Option<String>,
}

#[derive(Deserialize, Debug)]
struct LoginData {
    code: String,
    ticket: Option<String>,
    ok: Option<i16>,
    uin: Option<String>
}

fn get_headers() -> HeaderMap {
    let mut headers = HeaderMap::new();
    headers.insert("content-type", "application/json".parse().unwrap());
    headers.insert("qua", "V1_HT5_QDT_0.70.2209190_x64_0_DEV_D".parse().unwrap());
    headers.insert("host", "q.qq.com".parse().unwrap());
    headers.insert("accept", "application/json".parse().unwrap());
    headers
}

pub async fn get_auth_code(ticket: String) -> String{
    let client = Client::builder().build().unwrap();
    let url = format!("https://q.qq.com/ide/login");
    let auth = Auth {
        appid: String::from("1112386029"),
        ticket: ticket.clone()
    };
    let res = client.post(url).headers(get_headers()).json(&auth).send().await.unwrap();
    let text = res.text().await.unwrap();
    println!("{:#?}", text);
    let json = serde_json::from_str::<AuthCode>(text.as_str()).unwrap();
    json.code
}

#[derive(Deserialize, Debug)]
struct AuthCode {
    code: String,
    message: String,
}

#[derive(Deserialize, Serialize)]
struct Auth {
    appid: String,
    ticket: String,
}

async fn get_qq_number(message: Message) -> String{
    let mut qq_number= String::new();
    for segment in message.iter() {
        info!("segment = {:?}", segment);
        if segment.type_ == "at" {
            if let Some(qq) = segment.data.get("qq").and_then(|v| v.as_str()) {
                qq_number = qq.to_string();
            }
        }
    }
    if qq_number.is_empty() {
        return String::new()
    }
    qq_number
}
