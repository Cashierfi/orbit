use crate::DfxOrbit;
use anyhow::bail;
use candid::Principal;
use clap::Parser;
use orbit_essentials::cmc::SubnetSelection;
use station_api::{
    AllowDTO, AuthScopeDTO, CreateExternalCanisterOperationDTO,
    CreateExternalCanisterOperationInput, CreateExternalCanisterOperationKindAddExistingDTO,
    CreateExternalCanisterOperationKindCreateNewDTO, CreateExternalCanisterOperationKindDTO,
    ExternalCanisterPermissionsCreateInput, ExternalCanisterRequestPoliciesCreateInput,
    GetRequestResponse, RequestOperationDTO, RequestOperationInput,
};
use std::fmt::Write;

/// Requests the creation of a new canister, or registration of an existing one, in Orbit.
///
/// By default operates in `CreateNew` mode (station provisions a fresh canister).
/// Pass `--add-existing <CANISTER_ID>` to register an existing canister whose controller already
/// includes the Orbit station.
///
/// Permissions and request policies default to `Restricted` (empty users/groups, empty rules).
/// Tune these via the Orbit Web UI after creation.
#[derive(Debug, Clone, Parser)]
pub struct RequestCanisterCreateArgs {
    /// Display name for the canister within Orbit.
    pub name: String,

    /// Optional human-readable description.
    #[clap(long)]
    pub description: Option<String>,

    /// Label tag (repeatable).
    #[clap(long = "label")]
    pub labels: Vec<String>,

    /// Register an existing canister principal instead of creating a new one.
    /// Mutually exclusive with `--initial-cycles` and `--subnet`.
    #[clap(long, conflicts_with = "initial_cycles", conflicts_with = "subnet")]
    pub add_existing: Option<Principal>,

    /// Initial cycles to allocate (CreateNew mode only).
    /// Defaults to the station's minimum if omitted.
    #[clap(long)]
    pub initial_cycles: Option<u64>,

    /// Specific subnet principal on which to create the canister (CreateNew mode only).
    /// Defaults to the station's subnet if omitted.
    #[clap(long)]
    pub subnet: Option<Principal>,
}

impl RequestCanisterCreateArgs {
    /// Converts the CLI arg type into the equivalent Orbit API type.
    pub(crate) fn into_request(self) -> anyhow::Result<RequestOperationInput> {
        Ok(RequestOperationInput::CreateExternalCanister(
            self.into_input(),
        ))
    }

    pub(crate) fn verify(
        &self,
        _dfx_orbit: &DfxOrbit,
        request: &GetRequestResponse,
    ) -> anyhow::Result<()> {
        let RequestOperationDTO::CreateExternalCanister(op) = &request.request.operation else {
            bail!("This request is not a create external canister request");
        };

        let expected = self.clone().into_input();

        if op.input.name != expected.name {
            bail!(
                "Name mismatch: request='{}', local='{}'",
                op.input.name,
                expected.name
            );
        }
        if op.input.description != expected.description {
            bail!(
                "Description mismatch: request={:?}, local={:?}",
                op.input.description,
                expected.description
            );
        }
        if op.input.labels != expected.labels {
            bail!(
                "Labels mismatch: request={:?}, local={:?}",
                op.input.labels,
                expected.labels
            );
        }
        verify_kind(&op.input.kind, &expected.kind)?;

        Ok(())
    }

    /// Build the API input. `verify` calls this on a clone to compare against a pending request.
    fn into_input(self) -> CreateExternalCanisterOperationInput {
        let kind = if let Some(canister_id) = self.add_existing {
            CreateExternalCanisterOperationKindDTO::AddExisting(
                CreateExternalCanisterOperationKindAddExistingDTO { canister_id },
            )
        } else {
            CreateExternalCanisterOperationKindDTO::CreateNew(
                CreateExternalCanisterOperationKindCreateNewDTO {
                    initial_cycles: self.initial_cycles,
                    subnet_selection: self.subnet.map(|subnet| SubnetSelection::Subnet { subnet }),
                },
            )
        };

        CreateExternalCanisterOperationInput {
            kind,
            name: self.name,
            description: self.description,
            labels: if self.labels.is_empty() {
                None
            } else {
                Some(self.labels)
            },
            metadata: None,
            permissions: default_permissions(),
            request_policies: default_request_policies(),
        }
    }
}

/// Locked-down defaults: only admins (via station-wide default policies) can interact.
/// Users may broaden access in the Orbit Web UI after creation.
fn default_permissions() -> ExternalCanisterPermissionsCreateInput {
    let restricted = AllowDTO {
        auth_scope: AuthScopeDTO::Restricted,
        users: vec![],
        user_groups: vec![],
    };
    ExternalCanisterPermissionsCreateInput {
        read: restricted.clone(),
        change: restricted,
        calls: vec![],
    }
}

fn default_request_policies() -> ExternalCanisterRequestPoliciesCreateInput {
    ExternalCanisterRequestPoliciesCreateInput {
        change: vec![],
        calls: vec![],
    }
}

fn verify_kind(
    actual: &CreateExternalCanisterOperationKindDTO,
    expected: &CreateExternalCanisterOperationKindDTO,
) -> anyhow::Result<()> {
    match (actual, expected) {
        (
            CreateExternalCanisterOperationKindDTO::CreateNew(a),
            CreateExternalCanisterOperationKindDTO::CreateNew(e),
        ) => {
            if a.initial_cycles != e.initial_cycles {
                bail!(
                    "Initial cycles mismatch: request={:?}, local={:?}",
                    a.initial_cycles,
                    e.initial_cycles
                );
            }
            if a.subnet_selection != e.subnet_selection {
                bail!(
                    "Subnet selection mismatch: request={:?}, local={:?}",
                    a.subnet_selection,
                    e.subnet_selection
                );
            }
        }
        (
            CreateExternalCanisterOperationKindDTO::AddExisting(a),
            CreateExternalCanisterOperationKindDTO::AddExisting(e),
        ) => {
            if a.canister_id != e.canister_id {
                bail!(
                    "Canister id mismatch: request={}, local={}",
                    a.canister_id,
                    e.canister_id
                );
            }
        }
        _ => bail!("Create kind variant mismatch (CreateNew vs AddExisting)"),
    }
    Ok(())
}

impl DfxOrbit {
    pub(crate) fn display_create_canister_operation(
        &self,
        output: &mut String,
        op: &CreateExternalCanisterOperationDTO,
    ) -> anyhow::Result<()> {
        writeln!(output, "=== Create External Canister ===")?;
        writeln!(output, "Name: {}", op.input.name)?;
        if let Some(description) = &op.input.description {
            writeln!(output, "Description: {description}")?;
        }
        if let Some(labels) = &op.input.labels {
            if !labels.is_empty() {
                writeln!(output, "Labels: {}", labels.join(", "))?;
            }
        }

        match &op.input.kind {
            CreateExternalCanisterOperationKindDTO::CreateNew(create_new) => {
                writeln!(output, "Mode: CreateNew")?;
                if let Some(cycles) = create_new.initial_cycles {
                    writeln!(output, "Initial cycles: {cycles}")?;
                }
                if let Some(subnet_selection) = &create_new.subnet_selection {
                    writeln!(output, "Subnet selection: {subnet_selection:?}")?;
                }
            }
            CreateExternalCanisterOperationKindDTO::AddExisting(add_existing) => {
                writeln!(output, "Mode: AddExisting")?;
                writeln!(
                    output,
                    "Canister: {}",
                    self.try_reverse_lookup(&add_existing.canister_id)
                )?;
            }
        }

        // Populated by the station after execution; helpful for CreateNew where the canister id
        // is only known once the request executes successfully.
        if let Some(canister_id) = op.canister_id {
            writeln!(
                output,
                "Created canister: {}",
                self.try_reverse_lookup(&canister_id)
            )?;
        }

        Ok(())
    }
}
