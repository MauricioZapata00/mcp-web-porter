use crate::types::HtmlError;
use rusqlite::OpenFlags;
use std::path::PathBuf;
use url::Url;

#[derive(Clone, Debug)]
pub(crate) struct Cookie {
    pub host_key: String,
    pub name: String,
    pub value: String,
    pub path: String,
    pub expires_utc: i64,
    pub is_secure: bool,
}

#[cfg_attr(test, mockall::automock)]
pub(crate) trait CookieExtractor {
    fn extract(&self, url: &str) -> Result<Vec<Cookie>, HtmlError>;
}

pub(crate) struct ChromeCookieExtractor;

impl CookieExtractor for ChromeCookieExtractor {
    fn extract(&self, url: &str) -> Result<Vec<Cookie>, HtmlError> {
        let parsed_url = Url::parse(url)
            .map_err(|_| HtmlError::InvalidUrl(format!("Invalid URL: {}", url)))?;

        let host = parsed_url.host_str()
            .ok_or_else(|| HtmlError::InvalidUrl("URL has no host".to_string()))?;

        let profile_path = default_cookies_path()
            .ok_or_else(|| HtmlError::CookieExtractionError(
                "Chrome profile not found. Expected at:\n\
                macOS: ~/Library/Application Support/Google/Chrome/Default/Cookies\n\
                Linux: ~/.config/google-chrome/Default/Cookies\n\
                Windows: %LOCALAPPDATA%\\Google\\Chrome\\User Data\\Default\\Network\\Cookies\n\n\
                Verify Google Chrome is installed, or use 'debug_port' to connect to a running Chrome instance.".to_string()
            ))?;

        if !profile_path.exists() {
            return Err(HtmlError::CookieExtractionError(
                format!("Chrome cookies file not found at: {}", profile_path.display())
            ));
        }

        let conn = rusqlite::Connection::open_with_flags(
            &profile_path,
            OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
        ).map_err(|e| HtmlError::CookieExtractionError(
            format!("Could not read Chrome cookies database: {}", e)
        ))?;

        let mut stmt = conn.prepare(
            "SELECT host_key, name, encrypted_value, value, path, expires_utc, is_secure \
             FROM cookies WHERE host_key LIKE '%' || ?1 || '%'"
        ).map_err(|e| HtmlError::CookieExtractionError(
            format!("Could not query cookies: {}", e)
        ))?;

        let cookies = stmt.query_map([host], |row| {
            let encrypted_value: Vec<u8> = row.get(2)?;
            let plaintext_value: String = row.get(3)?;

            let decrypted_value = if encrypted_value.is_empty() {
                plaintext_value
            } else {
                decrypt_cookie_value(&encrypted_value)
                    .unwrap_or(plaintext_value)
            };

            Ok(Cookie {
                host_key: row.get(0)?,
                name: row.get(1)?,
                value: decrypted_value,
                path: row.get(4)?,
                expires_utc: row.get(5)?,
                is_secure: row.get::<_, i32>(6)? != 0,
            })
        }).map_err(|e| HtmlError::CookieExtractionError(
            format!("Error iterating cookies: {}", e)
        ))?;

        let mut result = Vec::new();
        for cookie_result in cookies {
            match cookie_result {
                Ok(cookie) => result.push(cookie),
                Err(e) => eprintln!("Warning: failed to read cookie row: {}", e),
            }
        }

        Ok(result)
    }
}

pub(crate) fn default_cookies_path() -> Option<PathBuf> {
    #[cfg(target_os = "macos")]
    {
        dirs::home_dir().map(|h| {
            h.join("Library/Application Support/Google/Chrome/Default/Cookies")
        })
    }
    #[cfg(target_os = "linux")]
    {
        dirs::home_dir().map(|h| h.join(".config/google-chrome/Default/Cookies"))
    }
    #[cfg(target_os = "windows")]
    {
        std::env::var("LOCALAPPDATA").ok().map(|p| {
            PathBuf::from(p).join("Google/Chrome/User Data/Default/Network/Cookies")
        })
    }
    #[cfg(not(any(target_os = "macos", target_os = "linux", target_os = "windows")))]
    {
        None
    }
}

pub(crate) fn chrome_expires_to_unix_seconds(expires_utc: i64) -> Option<f64> {
    const CHROME_TO_UNIX_EPOCH_SECONDS: f64 = 11_644_473_600.0;

    if expires_utc <= 0 {
        return None;
    }

    let unix_seconds = (expires_utc as f64 / 1_000_000.0) - CHROME_TO_UNIX_EPOCH_SECONDS;
    (unix_seconds > 0.0).then_some(unix_seconds)
}

fn decrypt_cookie_value(encrypted: &[u8]) -> Option<String> {
    #[cfg(target_os = "macos")]
    {
        decrypt_cookie_value_macos(encrypted)
    }
    #[cfg(target_os = "linux")]
    {
        decrypt_cookie_value_linux(encrypted)
    }
    #[cfg(target_os = "windows")]
    {
        decrypt_cookie_value_windows(encrypted)
    }
    #[cfg(not(any(target_os = "macos", target_os = "linux", target_os = "windows")))]
    {
        None
    }
}

#[cfg(target_os = "macos")]
fn decrypt_cookie_value_macos(encrypted: &[u8]) -> Option<String> {
    use security_framework::passwords::{generic_password, PasswordOptions};

    if encrypted.len() < 3 {
        return None;
    }

    let prefix = &encrypted[..3];
    if prefix != b"v10" && prefix != b"v11" {
        return None;
    }

    let cipher_text = &encrypted[3..];

    let password = match generic_password(PasswordOptions::new_generic_password(
        "Chrome Safe Storage",
        "Chrome",
    )) {
        Ok(pwd) => pwd,
        Err(_) => {
            eprintln!("Warning: could not retrieve Chrome Safe Storage password from Keychain");
            return None;
        }
    };

    let password_str = match String::from_utf8(password) {
        Ok(s) => s,
        Err(_) => return None,
    };

    let key = derive_pbkdf2_key(password_str.as_bytes(), b"saltysalt", 1003, 16);

    aes_cbc_decrypt(&key, cipher_text)
}

#[cfg(target_os = "linux")]
fn decrypt_cookie_value_linux(encrypted: &[u8]) -> Option<String> {
    if encrypted.len() < 3 {
        return None;
    }

    let prefix = &encrypted[..3];
    if prefix != b"v10" && prefix != b"v11" {
        return None;
    }

    let cipher_text = &encrypted[3..];
    let key = derive_pbkdf2_key(b"peanuts", b"saltysalt", 1, 16);

    aes_cbc_decrypt(&key, cipher_text)
}

#[cfg(target_os = "windows")]
fn decrypt_cookie_value_windows(encrypted: &[u8]) -> Option<String> {
    use windows::Win32::Security::Cryptography::{
        CryptUnprotectData, CRYPT_INTEGER_BLOB,
    };
    use windows::Win32::Foundation::LPSTR;

    unsafe {
        let mut cipher_blob = CRYPT_INTEGER_BLOB {
            cbData: encrypted.len() as u32,
            pbData: encrypted.as_ptr() as *mut u8,
        };

        let mut plain_blob = std::mem::zeroed::<CRYPT_INTEGER_BLOB>();

        if CryptUnprotectData(
            &mut cipher_blob,
            None,
            None,
            None,
            None,
            0,
            &mut plain_blob,
        ).is_ok() {
            let slice = std::slice::from_raw_parts(
                plain_blob.pbData as *const u8,
                plain_blob.cbData as usize,
            );
            let result = String::from_utf8_lossy(slice).to_string();

            if !plain_blob.pbData.is_null() {
                let _ = windows::Win32::Security::Cryptography::LocalFree(
                    windows::Win32::Foundation::HLOCAL(plain_blob.pbData as *mut _)
                );
            }

            Some(result)
        } else {
            None
        }
    }
}

fn derive_pbkdf2_key(password: &[u8], salt: &[u8], iterations: u32, dklen: usize) -> Vec<u8> {
    use pbkdf2::pbkdf2_hmac;
    use sha1::Sha1;

    let mut key = vec![0u8; dklen];
    pbkdf2_hmac::<Sha1>(password, salt, iterations, &mut key);
    key
}

fn aes_cbc_decrypt(key: &[u8], cipher_text: &[u8]) -> Option<String> {
    if key.len() != 16 || cipher_text.len() % 16 != 0 {
        return None;
    }

    use aes::Aes128;
    use cbc::Decryptor;
    use cipher::{KeyIvInit, BlockModeDecrypt};

    let iv = [b' '; 16];
    let decryptor = Decryptor::<Aes128>::new_from_slices(key, &iv).ok()?;

    let mut buf = cipher_text.to_vec();
    let plaintext = decryptor.decrypt_padded::<cipher::block_padding::Pkcs7>(&mut buf).ok()?;

    String::from_utf8(plaintext.to_vec()).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_cookies_path_exists() {
        let path = default_cookies_path();
        assert!(path.is_some());
        let p = path.unwrap();
        assert!(p.to_string_lossy().contains("Chrome") || p.to_string_lossy().contains("google-chrome"));
    }

    #[test]
    fn test_pbkdf2_key_derivation() {
        let key = derive_pbkdf2_key(b"peanuts", b"saltysalt", 1, 16);
        assert_eq!(key.len(), 16);
    }
}
