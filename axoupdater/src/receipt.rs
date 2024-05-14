use std::{
    env::{self, current_dir, current_exe},
    path::PathBuf,
};

use crate::{errors::*, AxoUpdater, ReleaseSource};
use axoasset::SourceFile;
use axotag::Version;
use camino::Utf8PathBuf;
use serde::Deserialize;

/// Information parsed from a cargo-dist install receipt
#[derive(Clone, Debug, Deserialize)]
pub struct InstallReceipt {
    /// The path this app has been installed to
    pub install_prefix: Utf8PathBuf,
    /// A list of binaries installed by this app
    pub binaries: Vec<String>,
    /// Information about where this release was fetched from
    pub source: ReleaseSource,
    /// Installed version
    pub version: String,
    /// Information about the tool used to produce this receipt
    pub provider: ReceiptProvider,
}

/// Tool used to produce this install receipt
#[derive(Clone, Debug, Deserialize)]
pub struct ReceiptProvider {
    /// The name of the tool used to create this receipt
    pub source: String,
    /// The version of the above tool
    pub version: String,
}

impl AxoUpdater {
    /// Attempts to load an install receipt in order to prepare for an update.
    /// If present and valid, the install receipt is used to populate the
    /// `source` and `current_version` fields.
    /// Shell and Powershell installers produced by cargo-dist since 0.9.0
    /// will have created an install receipt.
    pub fn load_receipt(&mut self) -> AxoupdateResult<&mut AxoUpdater> {
        let Some(app_name) = &self.name else {
            return Err(AxoupdateError::NoAppNamePassed {});
        };

        let receipt = load_receipt_for(app_name)?;

        self.source = Some(receipt.source);
        self.current_version = Some(receipt.version.parse::<Version>()?);

        let provider = crate::Provider {
            source: receipt.provider.source,
            version: receipt.provider.version.parse::<Version>()?,
        };

        self.current_version_installed_by = Some(provider);
        self.install_prefix = Some(receipt.install_prefix);

        Ok(self)
    }

    /// Checks to see if the loaded install receipt is for this executable.
    /// Used to guard against cases where the running EXE is from a package
    /// manager, but a receipt from a shell installed-copy is present on the
    /// system.
    /// Returns an error if the receipt hasn't been loaded yet.
    pub fn check_receipt_is_for_this_executable(&self) -> AxoupdateResult<bool> {
        let current_exe_path = Utf8PathBuf::from_path_buf(current_exe()?.canonicalize()?)
            .map_err(|path| AxoupdateError::CaminoConversionFailed { path })?;
        // First determine the parent dir
        let mut current_exe_root = if let Some(parent) = current_exe_path.parent() {
            parent.to_path_buf()
        } else {
            current_exe_path
        };

        let receipt_root = self.install_prefix_root_normalized()?;

        // If the parent dir is a "bin" dir, strip it to get the true root,
        // but only if the true install root isn't itself a `bin` dir.
        if current_exe_root.file_name() == Some("bin") && receipt_root.file_name() != Some("bin") {
            if let Some(parent) = current_exe_root.parent() {
                current_exe_root = parent.to_path_buf();
            }
        }

        // Looks like this EXE comes from a different source than the install
        // receipt
        if current_exe_root != receipt_root {
            return Ok(false);
        }

        Ok(true)
    }
}

pub(crate) fn get_config_path(app_name: &str) -> AxoupdateResult<Utf8PathBuf> {
    if env::var("AXOUPDATER_CONFIG_WORKING_DIR").is_ok() {
        Ok(Utf8PathBuf::try_from(current_dir()?)?)
    } else if let Ok(path) = env::var("AXOUPDATER_CONFIG_PATH") {
        Ok(Utf8PathBuf::from(path))
    } else {
        let home = if cfg!(windows) {
            env::var("LOCALAPPDATA").map(PathBuf::from).ok()
        } else {
            homedir::get_my_home()?.map(|path| path.join(".config"))
        };
        let Some(home) = home else {
            return Err(AxoupdateError::NoHome {});
        };

        Ok(Utf8PathBuf::try_from(home)?.join(app_name))
    }
}

fn load_receipt_from_path(install_receipt_path: &Utf8PathBuf) -> AxoupdateResult<InstallReceipt> {
    Ok(SourceFile::load_local(install_receipt_path)?.deserialize_json()?)
}

fn load_receipt_for(app_name: &str) -> AxoupdateResult<InstallReceipt> {
    let Ok(receipt_prefix) = get_config_path(app_name) else {
        return Err(AxoupdateError::ConfigFetchFailed {
            app_name: app_name.to_owned(),
        });
    };

    let install_receipt_path = receipt_prefix.join(format!("{app_name}-receipt.json"));

    load_receipt_from_path(&install_receipt_path).map_err(|_| AxoupdateError::ReceiptLoadFailed {
        app_name: app_name.to_owned(),
    })
}
