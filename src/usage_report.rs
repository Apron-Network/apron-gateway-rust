use std::collections::HashMap;
use std::ops::Deref;
use std::time::{SystemTime, UNIX_EPOCH};

use crate::forward_service_models::ProxyRequestInfo;
use crate::HttpProxyResponse;

#[derive(Clone)]
pub struct UsageReportManager {
    pub account_reports: HashMap<String, UsageReport>,
}

impl UsageReportManager {
    // harvest collect data from all recorded accounts, clear all records and wait for new round of accounting
    fn harvest(&self) {}

    fn add_record(&mut self, account_id: String, data_size: u128, is_upload: bool) {
        let report = self
            .account_reports
            .entry(account_id.clone())
            .or_insert(UsageReport::new(account_id.clone()));

        report.record_usage(1, data_size, is_upload);
    }

    pub fn add_record_from_proxy_request_info(&mut self, req_info: &ProxyRequestInfo) {
        self.account_reports
            .entry(req_info.clone().user_key)
            .or_insert(UsageReport::new(req_info.clone().user_key))
            .record_usage(1, req_info.clone().raw_body.len() as u128, true);
    }

    pub fn add_record_from_http_proxy_response(&mut self, proxy_resp: &HttpProxyResponse) {
        self.account_reports
            .entry(req_info.clone().user_key)
            .or_insert(UsageReport::new(req_info.clone().user_key))
            .record_usage(0, proxy_resp.body.len() as u128, false);
    }
}

#[derive(Clone)]
pub struct UsageReport {
    pub account_id: String,
    pub start_timestamp: u128,
    pub end_timestamp: u128,
    pub access_count: u32,
    pub upload_traffic: u128,
    pub download_traffic: u128,
}

impl UsageReport {
    fn new(account_id: String) -> UsageReport {
        let current_ts = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("Clock may have gone backwards")
            .as_micros();
        return UsageReport {
            account_id,
            start_timestamp: current_ts,
            end_timestamp: current_ts,
            access_count: 0,
            upload_traffic: 0,
            download_traffic: 0,
        };
    }

    fn finalize(&mut self) -> &mut UsageReport {
        self.end_timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("Clock may have gone backwards")
            .as_micros();
        self
    }

    fn record_usage(&mut self, cnt: u32, data_size: u128, is_upload: bool) {
        self.access_count += cnt;

        if is_upload {
            self.upload_traffic += data_size;
        } else {
            self.download_traffic += data_size;
        }
    }
}
