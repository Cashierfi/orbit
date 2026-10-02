use crate::DfxOrbit;
use anyhow::Context;
use candid::Principal;
use slog::{info, Logger};

impl DfxOrbit {
    pub(super) fn try_reverse_lookup(&self, canister_id: &Principal) -> String {
        match self.canister_name(canister_id).ok() {
            Some(canister_name) => {
                format!("{canister_name} ({canister_id})")
            }
            None => format!("{canister_id}"),
        }
    }
}

pub(super) fn parse_arguments(
    arg_string: &Option<String>,
    arg_path: &Option<String>,
    raw_arg: &Option<String>,
    raw_arg_path: &Option<String>,
) -> anyhow::Result<Option<Vec<u8>>> {
    // TODO: It would be really nice to be able to use `blob_from_arguments(..)` here, as in dfx, to get all the nice things such as help composing the argument.
    // First try to read the argument file, if it was provided

    let candid = arg_path
        .as_ref()
        .map(std::fs::read_to_string)
        .transpose()?
        // Otherwise use the argument from the command line
        .or_else(|| arg_string.clone())
        // Parse the candid
        .map(|arg_string| {
            candid_parser::parse_idl_args(&arg_string)
                .with_context(|| "Invalid Candid values".to_string())?
                .to_bytes()
        })
        .transpose()?;

    // Raw hex can come from a file, which avoids the OS limit on command line argument length
    // (128 KiB per argument on Linux) for large encoded arguments.
    let raw_arg = raw_arg_path
        .as_ref()
        .map(|path| {
            std::fs::read_to_string(path)
                .with_context(|| format!("Could not read raw argument file {path}"))
        })
        .transpose()?
        .or_else(|| raw_arg.clone())
        .map(|hex_string| {
            hex::decode(hex_string.trim()).with_context(|| "Invalid hex-encoded argument")
        })
        .transpose()?;
    let arg = candid.or(raw_arg);
    Ok(arg)
}

pub(super) fn log_hashes(
    logger: &Logger,
    name: &str,
    local: &Option<String>,
    remote: &Option<String>,
) {
    info!(logger, "Hash mismatch of {}", name);
    info!(logger, "Request {}: {}", name, display_arg_checksum(remote));
    info!(logger, "Local {}:   {}", name, display_arg_checksum(local));
}

pub(super) fn display_arg_checksum(arg: &Option<String>) -> String {
    arg.as_ref()
        .map(|s| s.to_string())
        .unwrap_or(String::from("None"))
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use super::*;

    #[test]
    fn raw_arg_is_hex_decoded() {
        let arg = parse_arguments(&None, &None, &Some("2a000000".to_string()), &None).unwrap();
        assert_eq!(arg, Some(vec![0x2a, 0, 0, 0]));
    }

    #[test]
    fn raw_arg_file_is_read_trimmed_and_hex_decoded() {
        let path =
            std::env::temp_dir().join(format!("dfx-orbit-raw-arg-{}.hex", std::process::id()));
        std::fs::write(&path, "4449444c0000\n").unwrap();

        let arg = parse_arguments(
            &None,
            &None,
            &None,
            &Some(path.to_string_lossy().to_string()),
        );
        std::fs::remove_file(&path).unwrap();

        assert_eq!(arg.unwrap(), Some(candid::encode_args(()).unwrap()));
    }

    #[test]
    fn missing_raw_arg_file_is_an_error() {
        let arg = parse_arguments(
            &None,
            &None,
            &None,
            &Some("/nonexistent/arg.hex".to_string()),
        );
        assert!(arg.is_err());
    }

    #[test]
    fn invalid_hex_is_an_error() {
        let arg = parse_arguments(&None, &None, &Some("not-hex".to_string()), &None);
        assert!(arg.is_err());
    }
}
