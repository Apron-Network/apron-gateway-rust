use std::ops::Deref;
use std::time::{SystemTime, UNIX_EPOCH};

pub struct UsageReport {
    pub account_id: String,
    pub start_timestamp: u128,
    pub end_timestamp:u128,
    pub access_count: u32,
    pub upload_traffic: u128,
    pub download_traffic: u128,
}

impl UsageReport {
    fn new(account_id: String) -> UsageReport {
        let current_ts = SystemTime::now().duration_since(UNIX_EPOCH).expect("Clock may have gone backwards").as_micros();
        return UsageReport{
            account_id,
            start_timestamp: current_ts,
            end_timestamp: current_ts,
            access_count: 0,
            upload_traffic: 0,
            download_traffic: 0
        }
    }

    fn finalize(mut self) -> UsageReport {
        self.end_timestamp = SystemTime::now().duration_since(UNIX_EPOCH).expect("Clock may have gone backwards").as_micros();
        self
    }

    fn inc_count(mut self, cnt: u32) {
        self.access_count+=cnt;
    }

    fn inc_traffic(mut self, pack_size: u128, is_upload: bool)  {
        if is_upload {
            self.upload_traffic += pack_size;
        } else {
            self.download_traffic += pack_size;
        }
    }
}

