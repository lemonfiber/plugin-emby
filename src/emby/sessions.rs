use std::collections::BTreeMap;
use std::sync::Mutex;

use super::Opened;
use crate::credential::Credential;

type Key = (String, Vec<u8>);

/// The sessions this adapter holds, by the credential that opened each.
#[derive(Debug, Default)]
pub struct Sessions(Mutex<BTreeMap<Key, Opened>>);

impl Sessions {
    pub(super) fn held(&self, credential: &Credential) -> Option<Opened> {
        self.0.lock().ok()?.get(&filed(credential)).cloned()
    }

    pub(super) fn keep(&self, credential: &Credential, opened: &Opened) {
        if let Ok(mut held) = self.0.lock() {
            held.clear();
            held.insert(filed(credential), opened.clone());
        }
    }

    pub(super) fn forget(&self, credential: &Credential) {
        if let Ok(mut held) = self.0.lock() {
            held.remove(&filed(credential));
        }
    }
}

fn filed(credential: &Credential) -> Key {
    let digest = ring::digest::digest(&ring::digest::SHA256, credential.password.as_bytes());
    (credential.username.clone(), digest.as_ref().to_vec())
}
