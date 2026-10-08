//! API keys in Windows Credential Manager, bound to the connection's exact destination.

use opendictate_core::model::SpeechConnection;

#[derive(Clone, Copy)]
pub enum Purpose {
    Speech,
    Notes,
}

impl Purpose {
    fn service(self) -> &'static str {
        match self {
            Self::Speech => "OpenDictate speech API",
            Self::Notes => "OpenDictate notes API",
        }
    }
}

fn entry(purpose: Purpose, connection: &SpeechConnection) -> Result<keyring::Entry, String> {
    keyring::Entry::new(purpose.service(), &connection.credential_account()).map_err(|e| e.to_string())
}

pub fn read(purpose: Purpose, connection: &SpeechConnection) -> Result<Option<String>, String> {
    match entry(purpose, connection)?.get_password() {
        Ok(key) => Ok(Some(key)),
        Err(keyring::Error::NoEntry) => Ok(None),
        Err(e) => Err(format!("Couldn't read the API key from Windows Credential Manager: {e}")),
    }
}

/// An empty key deletes the stored credential.
pub fn save(purpose: Purpose, connection: &SpeechConnection, key: &str) -> Result<(), String> {
    let entry = entry(purpose, connection)?;
    if key.is_empty() {
        match entry.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
            Err(e) => Err(format!("Couldn't remove the API key from Windows Credential Manager: {e}")),
        }
    } else {
        entry.set_password(key).map_err(|e| format!("Couldn't save the API key in Windows Credential Manager: {e}"))
    }
}
