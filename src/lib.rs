/// 京东首页->领京豆
use anyhow::Result;
use jd_com::account::JAccount;
use jd_com::sign::get_sign;
use log::info;
use serde_json::{json, Value};
use std::time::Duration;

use reqwest::{
    header::{HeaderMap, HeaderValue},
    Client,
};

pub struct JClient {
    client: Client,
    base_url: String,
    account: JAccount,
}

impl JClient {
    pub fn new(account: JAccount) -> Self {
        let mut headers = HeaderMap::new();

        headers.append(
            "cookie",
            HeaderValue::from_str(account.cookie().as_str()).unwrap(),
        );

        let client = Client::builder()
            .default_headers(headers)
            .user_agent("JD4iPhone/168328%20(iPhone;%20iOS;%20Scale/3.00)")
            .build()
            .unwrap();
        let base_url = "https://api.m.jd.com/client.action".to_string();
        Self {
            client,
            base_url,
            account,
        }
    }

    async fn do_sub_task(&self, action_type: u8, task_token: &String) -> Result<bool> {
        let body = json!({
            "actionType": action_type,
            "taskToken": task_token,
        })
        .to_string();
        let data = self.request("beanDoTask", body.as_str()).await?;
        Ok(data.is_some())
    }

    async fn request(&self, function_id: &str, body: &str) -> Result<Option<Value>> {
        let sign = get_sign(function_id, body);
        let url = format!("{}?{}", self.base_url, sign);
        let data = self
            .client
            .post(url)
            .body(format!("body={:?}", body))
            .send()
            .await?
            .json::<Value>()
            .await?;
        if data.get("code").is_none() || data.get("code").unwrap() != "0" {
            Ok(None)
        } else {
            Ok(Some(data.get("data").unwrap_or(&Value::Null).clone()))
        }
    }

    async fn sign_in(&self) -> Result<()> {
        let data = self.request("signBeanAct", r#"{"rnVersion":"4.7","fp":"-1","eid":"","shshshfp":"-1","userAgent":"-1","shshshfpa":"-1","referUrl":"-1","jda":"-1"}"#).await?;

        if data.is_none() {
            info!("签到失败.");
            return Ok(());
        }

        let status = data.unwrap()["status"].to_string().replace('"', "");

        match status.as_str() {
            "1" | "2" => info!("{}, 签到成功...", self.account.name()),
            _ => info!("{}, 签到失败...", self.account.name()),
        }
        Ok(())
    }

    async fn get_task_list(&self) -> Result<Vec<Value>> {
        let mut task_list: Vec<Value> = Vec::new();
        let data = self
            .request(
                "beanTaskList",
                r#"{"viewChannel":"wojing3","beanVersion":1}"#,
            )
            .await?;
        if data.is_none() {
            return Ok(task_list);
        }
        let item_list = data.unwrap()["taskInfos"].to_owned();
        let item_list = item_list.as_array();
        if item_list.is_none() {
            return Ok(task_list);
        };

        for item in item_list.unwrap() {
            task_list.push(item.to_owned());
        }
        Ok(task_list)
    }

    async fn do_tasks(&self) -> Result<()> {
        let task_list = self.get_task_list().await?;

        for task in task_list {
            let status = task["status"].as_u64().unwrap();
            let name = task["taskName"].as_str().unwrap();
            if status == 2 {
                info!("{}, 任务《{}》已完成...", self.account.name(), name);
                continue;
            }
            let wait_duration = task["waitDuration"].as_u64().unwrap();
            let max_times = task["maxTimes"].as_u64().unwrap();
            let times = task["times"].as_u64().unwrap();
            let task_id = task["taskId"].as_u64().unwrap();

            for _ in times..max_times {
                let item_list = self.get_task_list().await?;
                for item in item_list {
                    let tid = item["taskId"].as_u64().unwrap();
                    if tid == task_id {
                        let task_token = item["subTaskVOS"][0]["taskToken"]
                            .to_string()
                            .replace('"', "");

                        let sub_name = item["subTaskVOS"][0]["title"].as_str().unwrap();
                        let _ = self.do_sub_task(1, &task_token).await;
                        info!(
                            "{}, 任务:{}->{}进行中, 等待{}秒...",
                            self.account.name(),
                            name,
                            sub_name,
                            wait_duration
                        );
                        tokio::time::sleep(Duration::from_secs(wait_duration)).await;
                        let bool = self.do_sub_task(0, &task_token).await;
                        if bool.is_ok() && bool.unwrap() {
                            info!(
                                "{}, 任务:{}->{}, 已完成...",
                                self.account.name(),
                                name,
                                sub_name
                            );
                        } else {
                            info!(
                                "{}, 任务:{}->{}, 失败...",
                                self.account.name(),
                                name,
                                sub_name
                            );
                        }
                    }
                }

                tokio::time::sleep(Duration::from_secs(2)).await;
            }
        }
        Ok(())
    }

    pub async fn run(&self) -> Result<()> {
        self.sign_in().await?;
        self.do_tasks().await
    }
}
