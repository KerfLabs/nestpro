//! FulcrumPro (ERP/MES) connector, Public API v1
//! (https://api.fulcrumpro.com/swagger/v1/swagger.json; Fulcrum rules F… in MECHANICS.md).
//!
//! Fulcrum has endpoints made for nesting programs:
//! - `POST /api/nesting/nestable-jobs/list`: work waiting to be nested, per
//!   material shape + operation;
//! - `POST /api/nesting/work-package-manifest`: the cut list for chosen job
//!   operations, with the DXF attachment ids;
//! - `POST /api/work-orders`: a work order from those job operations;
//! - `PUT /api/work-orders/{wo}/operations/{op}/nesting-setup`: the nest
//!   (sheets, parts per sheet, quantity per job operation, unplaced parts).

use super::http::Client;
use super::*;
use serde::{Deserialize, Serialize};

pub const DEFAULT_URL: &str = "https://api.fulcrumpro.com";
/// Largest page the API accepts.
const PAGE: usize = 500;
/// The manifest takes 1–500 job operations per request.
const MANIFEST_MAX: usize = 500;
/// A work order takes at most 500 job operations.
const WORK_ORDER_MAX: usize = 500;

/// Values Fulcrum accepts for `nestingSoftware` (WorkOrderNestingSetupNestingSoftwareEnum).
pub const SOFTWARE: [&str; 11] = ["unknown", "proNest", "nestingCenter", "radan", "lantek", "drAbe", "boost", "sigmaNest", "cadman", "fulcrum", "metamation"];

pub struct Fulcrum {
    http: Client,
    units: Units,
    approve: bool,
    software: String,
}

impl Fulcrum {
    pub fn new(base: &str, token: String, units: Units, approve: bool, software: &str) -> Self {
        let software = SOFTWARE.iter().find(|s| s.eq_ignore_ascii_case(software)).unwrap_or(&"unknown").to_string();
        Fulcrum { http: Client::new(base, token, "Fulcrum"), units, approve, software }
    }

    /// Find a catalog item's id by its number (exact, any case).
    pub fn item_id_by_number(&self, number: &str) -> Result<Option<String>, String> {
        let body = serde_json::json!({ "numbers": [{ "query": number.trim(), "mode": "equal", "casingOption": "caseInsensitive" }], "latestRevision": true });
        let v: Vec<ItemListDto> = self.http.post_json("/api/items/list/v2", &[("Skip", "0".into()), ("Take", "5".into())], &body)?;
        Ok(v.into_iter().find(|i| i.number.trim().eq_ignore_ascii_case(number.trim())).map(|i| i.id))
    }
}

// ---- Fulcrum's JSON (only the fields used; everything optional) ----

#[derive(Deserialize, Default, Debug, Clone)]
#[serde(rename_all = "camelCase", default)]
pub struct RefDto {
    pub id: Option<String>,
    pub name: Option<String>,
}

/// A reference Fulcrum may send as `null` (a job with no sales order, a step
/// with no scheduled equipment): read as an empty one instead of refusing the
/// whole answer (F34).
fn ref_or_null<'de, D: serde::Deserializer<'de>>(d: D) -> Result<RefDto, D::Error> {
    Ok(Option::<RefDto>::deserialize(d)?.unwrap_or_default())
}

impl RefDto {
    fn to_ref(&self) -> Ref {
        Ref { id: self.id.clone().unwrap_or_default(), name: self.name.clone().unwrap_or_default() }
    }
}

#[derive(Deserialize, Default, Debug)]
#[serde(rename_all = "camelCase", default)]
pub struct Paged<T> {
    pub data: Vec<T>,
    pub has_next_page: bool,
    pub total_count: i64,
}

#[derive(Deserialize, Default, Debug)]
#[serde(rename_all = "camelCase", default)]
pub struct ValidateDto {
    pub name: String,
    pub domain: String,
    pub expiration_utc: String,
}

#[derive(Deserialize, Default, Debug)]
#[serde(rename_all = "camelCase", default)]
pub struct GroupDto {
    #[serde(deserialize_with = "ref_or_null")]
    pub material: RefDto,
    #[serde(deserialize_with = "ref_or_null")]
    pub operation: RefDto,
    pub earliest_scheduled_start_utc: Option<String>,
    pub job_operations: Vec<GroupJobOpDto>,
    pub work_order_operations: Vec<GroupWoOpDto>,
}

#[derive(Deserialize, Default, Debug)]
#[serde(rename_all = "camelCase", default)]
pub struct GroupJobOpDto {
    pub job_operation_id: String,
    #[serde(deserialize_with = "ref_or_null")]
    pub job: RefDto,
    #[serde(deserialize_with = "ref_or_null")]
    pub item: RefDto,
    pub item_to_make_id: Option<String>,
    pub quantity_to_make: f64,
    pub customer_name: Option<String>,
    #[serde(deserialize_with = "ref_or_null")]
    pub sales_order: RefDto,
    pub production_due_date_utc: Option<String>,
    pub scheduled_start_utc: Option<String>,
    pub is_rework: bool,
    pub has_multiple_materials: bool,
}

#[derive(Deserialize, Default, Debug)]
#[serde(rename_all = "camelCase", default)]
pub struct GroupWoOpDto {
    pub work_order_operation_id: String,
    #[serde(deserialize_with = "ref_or_null")]
    pub work_order: RefDto,
    pub status: Option<String>,
    pub operation_status: Option<String>,
    pub sheet_count: u32,
    pub part_count: u32,
    pub machine_time_seconds: f64,
    pub is_nested: bool,
}

#[derive(Deserialize, Default, Debug)]
#[serde(rename_all = "camelCase", default)]
pub struct ManifestDto {
    pub parts: Vec<ManifestPartDto>,
    pub skipped: Vec<SkippedDto>,
}

#[derive(Deserialize, Default, Debug)]
#[serde(rename_all = "camelCase", default)]
pub struct ManifestPartDto {
    pub job_operation_id: String,
    #[serde(deserialize_with = "ref_or_null")]
    pub job: RefDto,
    pub customer_name: Option<String>,
    pub production_due_date_utc: Option<String>,
    #[serde(deserialize_with = "ref_or_null")]
    pub item: RefDto,
    pub item_number: Option<String>,
    pub item_revision: Option<String>,
    pub item_to_make_id: String,
    pub quantity_to_make: f64,
    pub work_order_operation_id: Option<String>,
    pub is_rework: bool,
    pub has_multiple_materials: bool,
    pub material: Option<MaterialDto>,
    pub drawings: Vec<DrawingDto>,
}

#[derive(Deserialize, Default, Debug)]
#[serde(rename_all = "camelCase", default)]
pub struct MaterialDto {
    pub material_shape_id: String,
    pub name: String,
    pub form: Option<String>,
    pub material_name: Option<String>,
    pub grade: Option<String>,
    pub thickness: Option<f64>,
    pub thickness_description: Option<String>,
    pub dimension_units: Option<String>,
}

#[derive(Deserialize, Default, Debug)]
#[serde(rename_all = "camelCase", default)]
pub struct DrawingDto {
    pub attachment_id: String,
    pub file_name: String,
    pub owner_type: Option<String>,
    pub is_operation_specific: bool,
}

#[derive(Deserialize, Default, Debug)]
#[serde(rename_all = "camelCase", default)]
pub struct SkippedDto {
    pub job_id: String,
    pub job_operation_id: String,
    pub reason: String,
}

#[derive(Deserialize, Default, Debug)]
#[serde(rename_all = "camelCase", default)]
pub struct CreatedDto {
    pub id: String,
}

#[derive(Deserialize, Default, Debug)]
#[serde(rename_all = "camelCase", default)]
pub struct WoOpDto {
    pub id: String,
    pub order: i32,
    pub status: Option<String>,
    #[serde(deserialize_with = "ref_or_null")]
    pub operation: RefDto,
    #[serde(deserialize_with = "ref_or_null")]
    pub scheduled_equipment: RefDto,
    pub scheduled_start_utc: Option<String>,
    pub is_nested: bool,
    pub job_operations: Vec<WoOpJobOpDto>,
}

#[derive(Deserialize, Default, Debug)]
#[serde(rename_all = "camelCase", default)]
pub struct WoOpJobOpDto {
    pub job_id: String,
    pub job_number: Option<i64>,
    pub job_name: Option<String>,
    pub item_to_make_id: Option<String>,
    pub job_operation_id: String,
}

#[derive(Deserialize, Default, Debug)]
#[serde(rename_all = "camelCase", default)]
pub struct OperationDto {
    pub id: String,
    pub name: String,
    pub is_nestable: bool,
    pub work_centers: Vec<RefDto>,
    pub equipment: Vec<RefDto>,
}

pub fn step_from(o: WoOpDto) -> WorkOrderStep {
    WorkOrderStep {
        id: o.id,
        order: o.order,
        status: s(&o.status),
        operation: o.operation.to_ref(),
        equipment: o.scheduled_equipment.to_ref(),
        start: s(&o.scheduled_start_utc),
        nested: o.is_nested,
        job_ops: o
            .job_operations
            .into_iter()
            .map(|j| {
                let name = j.job_name.filter(|n| !n.trim().is_empty()).or(j.job_number.map(|n| n.to_string())).unwrap_or_default();
                (j.job_id, name, j.job_operation_id, j.item_to_make_id.unwrap_or_default())
            })
            .collect(),
    }
}

#[derive(Deserialize, Default, Debug)]
#[serde(rename_all = "camelCase", default)]
pub struct SaveResultDto {
    pub validation_issues: Vec<IssueDto>,
}

#[derive(Deserialize, Default, Debug)]
#[serde(rename_all = "camelCase", default)]
pub struct IssueDto {
    #[serde(rename = "type")]
    pub kind: String,
    pub severity: String,
    pub index: i32,
    pub message: String,
}

#[derive(Deserialize, Default, Debug)]
#[serde(rename_all = "camelCase", default)]
pub struct ItemListDto {
    pub id: String,
    pub number: String,
}

// ---- What is sent ----

#[derive(Serialize, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct NestingSetupSave {
    pub nesting_software: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub machine_name: Option<String>,
    pub parts: Vec<SavePart>,
    pub plates: Vec<SavePlate>,
    pub unplaced_parts: Vec<SaveUnplaced>,
}

#[derive(Serialize, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SavePart {
    pub part_number: String,
    pub quantity: u32,
    pub surface_area: f64,
    pub contours_length: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub item_id: Option<String>,
    pub job_operation_quantities: Vec<SaveOpQty>,
}

#[derive(Serialize, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SaveOpQty {
    pub job_operation_id: String,
    pub quantity: u32,
}

#[derive(Serialize, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SavePlate {
    pub dimension_x: f64,
    pub dimension_y: f64,
    pub runtime_seconds: f64,
    pub count: u32,
    pub page_number: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub item_id: Option<String>,
    pub sheet_name: String,
    pub parts: Vec<SavePlatePart>,
}

#[derive(Serialize, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SavePlatePart {
    pub part_number: String,
    pub quantity: u32,
}

#[derive(Serialize, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SaveUnplaced {
    #[serde(skip_serializing_if = "String::is_empty")]
    pub file_id: String,
    pub file_name: String,
    pub quantity: u32,
    pub reason: String,
}

/// Round to 4 decimals (Fulcrum shows what it is sent).
fn r4(x: f64) -> f64 {
    (x * 1e4).round() / 1e4
}

/// The nest in Fulcrum's form, in `units`; `sheet_item` is the stock item
/// the sheets are cut from, when known.
pub fn setup_body(nest: &NestReport, units: Units, software: &str, sheet_item: Option<&str>) -> NestingSetupSave {
    let u = units.mm();
    NestingSetupSave {
        nesting_software: software.to_string(),
        machine_name: Some(nest.machine.clone()).filter(|m| !m.trim().is_empty()),
        parts: nest
            .parts
            .iter()
            .map(|p| SavePart {
                part_number: p.number.clone(),
                quantity: p.qty,
                surface_area: r4(p.area_mm2 / (u * u)),
                contours_length: r4(p.contour_mm / u),
                item_id: Some(p.item_id.clone()).filter(|s| !s.is_empty()),
                job_operation_quantities: p.per_op.iter().map(|(_, op, k)| SaveOpQty { job_operation_id: op.clone(), quantity: *k }).collect(),
            })
            .collect(),
        plates: nest
            .sheets
            .iter()
            .enumerate()
            .map(|(k, s)| SavePlate {
                dimension_x: r4(s.w_mm / u),
                dimension_y: r4(s.h_mm / u),
                runtime_seconds: s.runtime_s.round(),
                count: s.count.max(1),
                page_number: k as u32 + 1,
                item_id: sheet_item.map(str::to_string),
                sheet_name: s.name.clone(),
                parts: s.parts.iter().map(|(n, q)| SavePlatePart { part_number: n.clone(), quantity: *q }).collect(),
            })
            .collect(),
        unplaced_parts: nest.unplaced.iter().map(|x| SaveUnplaced { file_id: x.file_id.clone(), file_name: x.file_name.clone(), quantity: x.qty, reason: x.reason.clone() }).collect(),
    }
}

fn s(o: &Option<String>) -> String {
    o.clone().unwrap_or_default()
}

pub fn group_from(g: GroupDto) -> Group {
    Group {
        material: g.material.to_ref(),
        operation: g.operation.to_ref(),
        earliest_start: s(&g.earliest_scheduled_start_utc),
        open: g
            .job_operations
            .into_iter()
            .map(|o| OpenOp {
                job: o.job.to_ref(),
                op_id: o.job_operation_id,
                item_to_make_id: s(&o.item_to_make_id),
                item: o.item.to_ref(),
                qty: o.quantity_to_make,
                customer: s(&o.customer_name),
                sales_order: s(&o.sales_order.name),
                due: s(&o.production_due_date_utc),
                start: s(&o.scheduled_start_utc),
                rework: o.is_rework,
                multi_material: o.has_multiple_materials,
            })
            .collect(),
        nests: g
            .work_order_operations
            .into_iter()
            .map(|w| NestRef { work_order: w.work_order.to_ref(), op_id: w.work_order_operation_id, status: s(&w.status), op_status: s(&w.operation_status), sheets: w.sheet_count, parts: w.part_count, machine_s: w.machine_time_seconds, nested: w.is_nested })
            .collect(),
    }
}

/// Thickness in mm from Fulcrum's value and unit name.
pub fn thickness_mm(t: Option<f64>, unit: Option<&str>) -> Option<f64> {
    let t = t.filter(|t| t.is_finite() && *t > 0.0)?;
    let f = match unit.unwrap_or("Inch").to_ascii_lowercase().as_str() {
        "millimeter" | "millimetre" | "mm" => 1.0,
        "centimeter" | "centimetre" | "cm" => 10.0,
        "meter" | "metre" | "m" => 1000.0,
        "foot" | "ft" => 304.8,
        "yard" | "yd" => 914.4,
        _ => 25.4,
    };
    Some(t * f)
}

pub fn package_from(m: ManifestDto) -> WorkPackage {
    let parts = m
        .parts
        .into_iter()
        .map(|p| {
            let job = p.job.to_ref();
            CutPart {
                demand: Demand {
                    provider: Provider::Fulcrum,
                    job_id: job.id.clone(),
                    job_name: job.name.clone(),
                    op_id: p.job_operation_id,
                    item_to_make_id: p.item_to_make_id,
                    item_id: s(&p.item.id),
                    qty: 0,
                    customer: s(&p.customer_name),
                    due: s(&p.production_due_date_utc),
                    drawing_id: String::new(),
                    drawing_name: String::new(),
                },
                qty: p.quantity_to_make,
                item: p.item.to_ref(),
                item_number: s(&p.item_number),
                revision: s(&p.item_revision),
                material: p.material.map(|m| MaterialInfo {
                    shape_id: m.material_shape_id,
                    name: m.name,
                    material_name: s(&m.material_name),
                    grade: s(&m.grade),
                    thickness_mm: thickness_mm(m.thickness, m.dimension_units.as_deref()),
                    thickness_text: s(&m.thickness_description),
                    form: s(&m.form),
                }),
                drawings: p.drawings.into_iter().map(|d| DrawingRef { id: d.attachment_id, file_name: d.file_name, op_specific: d.is_operation_specific }).collect(),
                rework: p.is_rework,
                multi_material: p.has_multiple_materials,
                nested_on: p.work_order_operation_id.filter(|x| !x.is_empty()),
            }
        })
        .collect();
    let skipped = m
        .skipped
        .into_iter()
        .map(|k| {
            let why = match k.reason.as_str() {
                "notFound" => "not found",
                "ambiguousJobOperation" => "the job has this operation on more than one item",
                "noSystemOperation" => "the operation isn't linked to a shop operation",
                r => r,
            };
            format!("Job {} operation {}: {why}", k.job_id, k.job_operation_id)
        })
        .collect();
    WorkPackage { parts, skipped }
}

fn issue_text(i: &IssueDto) -> String {
    let what = if i.kind == "plate" { format!("Sheet {}", i.index + 1) } else { format!("Part {}", i.index + 1) };
    let sev = if i.severity == "error" { "error" } else { "warning" };
    format!("{what} ({sev}): {}", i.message)
}

impl Connector for Fulcrum {
    fn check(&self) -> Result<String, String> {
        let v: ValidateDto = self.http.get_json("/api/auth/validate")?;
        let until = if v.expiration_utc.starts_with("9999") { "no expiry".to_string() } else { format!("valid until {}", day(&v.expiration_utc)) };
        Ok(format!("Signed in to {} with token \"{}\" ({until}).", v.domain, v.name))
    }

    fn groups(&self, f: &GroupFilter) -> Result<Vec<Group>, String> {
        let mut body = serde_json::json!({});
        if f.need_nesting {
            body["needNesting"] = true.into();
        }
        if f.flat_only {
            body["materialForms"] = serde_json::json!(["sheet", "plate"]);
        }
        if !f.search.trim().is_empty() {
            body["search"] = f.search.trim().into();
        }
        let mut out = vec![];
        for page in 0..50 {
            let q = [("Skip", (page * PAGE).to_string()), ("Take", PAGE.to_string())];
            let p: Paged<GroupDto> = self.http.post_json("/api/nesting/nestable-jobs/list", &q, &body)?;
            let n = p.data.len();
            out.extend(p.data.into_iter().map(group_from));
            if !p.has_next_page || n == 0 {
                break;
            }
        }
        Ok(out)
    }

    fn work_package(&self, ops: &[(String, String, String)]) -> Result<WorkPackage, String> {
        let mut all = WorkPackage::default();
        for chunk in ops.chunks(MANIFEST_MAX) {
            let list: Vec<serde_json::Value> = chunk
                .iter()
                .map(|(job, op, itm)| {
                    let mut v = serde_json::json!({ "jobId": job, "jobOperationId": op });
                    if !itm.is_empty() {
                        v["itemToMakeId"] = itm.clone().into();
                    }
                    v
                })
                .collect();
            let m: ManifestDto = self.http.post_json("/api/nesting/work-package-manifest", &[], &serde_json::json!({ "jobOperations": list }))?;
            let wp = package_from(m);
            all.parts.extend(wp.parts);
            all.skipped.extend(wp.skipped);
        }
        Ok(all)
    }

    fn download(&self, d: &DrawingRef) -> Result<Vec<u8>, String> {
        if d.id.len() != 24 || !d.id.chars().all(|c| c.is_ascii_alphanumeric()) {
            return Err(format!("{}: not a Fulcrum attachment id ({})", d.file_name, d.id));
        }
        self.http.get_bytes(&format!("/api/attachments/{}/download", d.id))
    }

    fn send_nest(&self, job_name: &str, link: &ErpJob, ops: &[(String, String)], nest: &NestReport) -> Result<SendResult, String> {
        if nest.parts.is_empty() {
            return Err("Nothing to send: no Fulcrum parts are placed on a sheet.".into());
        }
        let mut res = SendResult::default();
        let wo = match &link.work_order {
            Some(w) => w.clone(),
            None => {
                if ops.len() > WORK_ORDER_MAX {
                    return Err(format!("Fulcrum takes at most {WORK_ORDER_MAX} job operations per work order; this job has {}.", ops.len()));
                }
                let run: f64 = nest.sheets.iter().map(|s| s.runtime_s * s.count as f64).sum();
                let sheets: u32 = nest.sheets.iter().map(|s| s.count).sum();
                let body = serde_json::json!({
                    "name": job_name,
                    "status": if self.approve { "approved" } else { "needsReview" },
                    "operations": [{
                        "additionalInstructions": format!("Nested in Sheet Nesting: {sheets} sheet(s)."),
                        "estimatedRunTimeSeconds": run.round(),
                        "jobOperations": ops.iter().map(|(j, o)| serde_json::json!({ "jobId": j, "jobOperationId": o })).collect::<Vec<_>>(),
                    }],
                });
                let c: CreatedDto = self.http.post_json("/api/work-orders", &[], &body)?;
                res.created = true;
                let list: Vec<WoOpDto> = match self.http.post_json(&format!("/api/work-orders/{}/operations/list", c.id), &[], &serde_json::json!({})) {
                    Ok(l) => l,
                    Err(e) => {
                        res.work_order = Some(WorkOrderRef { id: c.id.clone(), op_id: String::new(), name: job_name.into() });
                        res.error = Some(format!("Work order made, but its operation couldn't be read: {e}"));
                        return Ok(res);
                    }
                };
                let first = ops.first().cloned().unwrap_or_default();
                let op = list.iter().find(|o| o.job_operations.iter().any(|j| j.job_operation_id == first.1 && (j.job_id.is_empty() || j.job_id == first.0))).or_else(|| list.iter().min_by_key(|o| o.order));
                let Some(op) = op else {
                    res.work_order = Some(WorkOrderRef { id: c.id.clone(), op_id: String::new(), name: job_name.into() });
                    res.error = Some("Work order made, but it has no operation to store the nest on.".into());
                    return Ok(res);
                };
                WorkOrderRef { id: c.id, op_id: op.id.clone(), name: job_name.into() }
            }
        };
        res.work_order = Some(wo.clone());
        if wo.op_id.is_empty() {
            res.error = Some("The work order's operation isn't known — forget the work order and send again.".into());
            return Ok(res);
        }
        // The sheet stock item, if one is named on the job.
        let sheet_item = if link.sheet_item.trim().is_empty() {
            None
        } else {
            match self.item_id_by_number(&link.sheet_item) {
                Ok(Some(id)) => Some(id),
                Ok(None) => {
                    res.issues.push(format!("No Fulcrum item numbered \"{}\" — sheets sent without their stock item.", link.sheet_item.trim()));
                    None
                }
                Err(e) => {
                    res.issues.push(format!("Couldn't look up the sheet item: {e}"));
                    None
                }
            }
        };
        let body = setup_body(nest, self.units, &self.software, sheet_item.as_deref());
        match self.http.put_json::<_, SaveResultDto>(&format!("/api/work-orders/{}/operations/{}/nesting-setup", wo.id, wo.op_id), &body) {
            Ok(r) => res.issues.extend(r.validation_issues.iter().map(issue_text)),
            Err(e) => res.error = Some(format!("Couldn't store the nest: {e}")),
        }
        Ok(res)
    }

    fn stations(&self) -> Result<Vec<StationInfo>, String> {
        let mut out = vec![];
        // A bare list: page until a page comes back short.
        for page in 0..20 {
            let q = [("Skip", (page * PAGE).to_string()), ("Take", PAGE.to_string())];
            let v: Vec<OperationDto> = self.http.post_json("/api/operations/list", &q, &serde_json::json!({}))?;
            let n = v.len();
            out.extend(v.into_iter().map(|o| StationInfo { op: Ref { id: o.id, name: o.name }, nestable: o.is_nestable, work_centers: o.work_centers.iter().map(RefDto::to_ref).collect(), equipment: o.equipment.iter().map(RefDto::to_ref).collect() }));
            if n < PAGE {
                break;
            }
        }
        out.sort_by(|a, b| b.nestable.cmp(&a.nestable).then(a.op.name.to_lowercase().cmp(&b.op.name.to_lowercase())));
        Ok(out)
    }

    fn work_order_steps(&self, work_order_id: &str) -> Result<Vec<WorkOrderStep>, String> {
        if work_order_id.len() != 24 || !work_order_id.chars().all(|c| c.is_ascii_alphanumeric()) {
            return Err(format!("not a Fulcrum work order id ({work_order_id})"));
        }
        let v: Vec<WoOpDto> = self.http.post_json(&format!("/api/work-orders/{work_order_id}/operations/list"), &[], &serde_json::json!({}))?;
        let mut steps: Vec<WorkOrderStep> = v.into_iter().map(step_from).collect();
        steps.sort_by_key(|s| s.order);
        Ok(steps)
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::integrations::mock::mock;

    pub(crate) const GROUPS: &str = r#"{"data":[{"material":{"id":"SS-304-Sheet-0.25","name":"304 SS 1/4\""},"operation":{"id":"op-laser","name":"Laser"},
      "earliestScheduledStartUtc":"2026-11-05T00:00:00Z",
      "jobOperations":[{"jobOperationId":"jo1","job":{"id":"6650aaaaaaaaaaaaaaaaaaaa","name":"J-1001"},"item":{"id":"it1","name":"Bracket"},"itemToMakeId":"itm1","quantityToMake":12,"customerName":"Acme","salesOrder":{"id":"so","name":"SO-55"},"productionDueDateUtc":"2026-11-10T00:00:00Z","scheduledStartUtc":null,"isRework":false,"hasMultipleMaterials":false,"futureField":1}],
      "workOrderOperations":[{"workOrderOperationId":"woop1","workOrder":{"id":"wo1","name":"WO-7"},"status":"approved","operationStatus":"Pending","priority":"low","sheetCount":3,"partCount":40,"machineTimeSeconds":900.5,"isNested":true}]}],
      "page":1,"pageSize":500,"totalCount":1,"totalPages":1,"hasPreviousPage":false,"hasNextPage":false}"#;

    pub(crate) const MANIFEST: &str = r#"{"parts":[{"jobOperationId":"jo1","job":{"id":"6650aaaaaaaaaaaaaaaaaaaa","name":"J-1001"},"customerName":"Acme","productionDueDateUtc":"2026-11-10T00:00:00Z",
      "item":{"id":"it1","name":"Bracket"},"itemNumber":"BRK-1","itemRevision":"B","itemToMakeId":"itm1","quantityToMake":12,"operation":{"id":"op-laser","name":"Laser"},
      "workOrderOperationId":null,"isRework":false,"hasMultipleMaterials":false,
      "material":{"materialShapeId":"SS-304-Sheet-0.25","name":"304 SS 1/4\"","form":"sheet","materialName":"Stainless Steel 304 Sheet","grade":"304","thickness":0.25,"thicknessDescription":"1/4\"","dimensionUnits":"Inch"},
      "drawings":[{"attachmentId":"6650bbbbbbbbbbbbbbbbbbbb","fileName":"BRK-1.dxf","ownerType":"item","isOperationSpecific":false}]}],
      "skipped":[{"jobId":"j9","jobOperationId":"jo9","reason":"noSystemOperation"}]}"#;

    #[test]
    fn nestable_groups_and_the_cut_list_are_read() {
        let p: Paged<GroupDto> = serde_json::from_str(GROUPS).unwrap();
        let g = group_from(p.data.into_iter().next().unwrap());
        assert_eq!((g.material.id.as_str(), g.operation.name.as_str()), ("SS-304-Sheet-0.25", "Laser"));
        assert_eq!((g.open[0].job.name.as_str(), g.open[0].qty, g.open[0].customer.as_str(), g.open[0].sales_order.as_str()), ("J-1001", 12.0, "Acme", "SO-55"));
        assert_eq!((g.nests[0].work_order.name.as_str(), g.nests[0].sheets, g.nests[0].nested), ("WO-7", 3, true));

        let wp = package_from(serde_json::from_str(MANIFEST).unwrap());
        let c = &wp.parts[0];
        assert_eq!(c.part_name(), "BRK-1 rev B");
        assert_eq!((c.demand.job_id.as_str(), c.demand.op_id.as_str(), c.demand.item_id.as_str()), ("6650aaaaaaaaaaaaaaaaaaaa", "jo1", "it1"));
        let m = c.material.as_ref().unwrap();
        assert!((m.thickness_mm.unwrap() - 6.35).abs() < 1e-9, "1/4 inch");
        assert_eq!(c.dxf().unwrap().id, "6650bbbbbbbbbbbbbbbbbbbb");
        assert!(wp.skipped[0].contains("isn't linked to a shop operation"));
        assert_eq!(thickness_mm(Some(3.0), Some("Millimeter")), Some(3.0));
        assert_eq!(thickness_mm(Some(0.0), Some("Inch")), None);
    }

    pub(crate) fn report() -> NestReport {
        NestReport {
            machine: "Laser 1".into(),
            parts: vec![NestPart { number: "BRK-1".into(), qty: 10, area_mm2: 645.16, contour_mm: 254.0, item_id: "it1".into(), per_op: vec![("j".into(), "jo1".into(), 10)] }],
            sheets: vec![NestSheet { w_mm: 3048.0, h_mm: 1524.0, count: 2, runtime_s: 300.4, name: "J-1".into(), parts: vec![("BRK-1".into(), 5)] }],
            unplaced: vec![Unplaced { file_id: "a".into(), file_name: "BRK-1.dxf".into(), qty: 2, reason: "10 of 12 placed".into() }],
            notes: vec![],
        }
    }

    #[test]
    fn the_nest_is_sent_in_inches_or_mm() {
        let b = setup_body(&report(), Units::Inch, "unknown", Some("sheetitem"));
        assert_eq!((b.parts[0].surface_area, b.parts[0].contours_length), (1.0, 10.0));
        assert_eq!((b.plates[0].dimension_x, b.plates[0].dimension_y, b.plates[0].count, b.plates[0].runtime_seconds, b.plates[0].page_number), (120.0, 60.0, 2, 300.0, 1));
        assert_eq!(b.plates[0].item_id.as_deref(), Some("sheetitem"));
        let v = serde_json::to_value(&b).unwrap();
        assert_eq!(v["parts"][0]["jobOperationQuantities"][0]["jobOperationId"], "jo1");
        assert_eq!(v["unplacedParts"][0]["quantity"], 2);
        assert_eq!(v["nestingSoftware"], "unknown");
        let mm = setup_body(&report(), Units::Millimeter, "lantek", None);
        assert_eq!((mm.plates[0].dimension_x, mm.parts[0].surface_area), (3048.0, 645.16));
        assert!(serde_json::to_value(&mm).unwrap()["plates"][0].get("itemId").is_none());
    }

    #[test]
    fn a_round_trip_against_a_stand_in_server() {
        let (url, seen_all) = mock(vec![
            ("/api/auth/validate", 200, r#"{"revocationId":"r","name":"Nesting PC","domain":"example","expirationUtc":"9999-12-31T23:59:59Z"}"#.into()),
            ("/api/nesting/nestable-jobs/list", 200, GROUPS.into()),
            ("/api/nesting/work-package-manifest", 200, MANIFEST.into()),
            ("/api/attachments/6650bbbbbbbbbbbbbbbbbbbb/download", 200, "0\nEOF\n".into()),
            ("/api/work-orders", 200, r#"{"id":"6650cccccccccccccccccccc"}"#.into()),
            ("/operations/list", 200, r#"[{"id":"woop9","order":1,"status":"pending","isNested":false,"jobOperations":[{"jobId":"6650aaaaaaaaaaaaaaaaaaaa","jobNumber":1001,"jobOperationId":"jo1"}]}]"#.into()),
            ("/api/items/list/v2", 200, r#"[{"id":"sheet-item-id","number":"SHT-304-14"}]"#.into()),
            ("/nesting-setup", 200, r#"{"nestingSetup":{"workOrderId":"x","workOrderOperationId":"woop9","isNested":true,"machineTimeSeconds":1,"parts":[],"plates":[],"unplacedParts":[]},"validationIssues":[{"type":"plate","severity":"warning","index":0,"message":"Runtime looks short"}]}"#.into()),
        ]);
        let f = Fulcrum::new(&url, "tok123".into(), Units::Inch, false, "lantek");
        assert_eq!(f.check().unwrap(), "Signed in to example with token \"Nesting PC\" (no expiry).");
        let g = f.groups(&GroupFilter { need_nesting: true, flat_only: true, search: "J-1001".into() }).unwrap();
        assert_eq!(g.len(), 1);
        let wp = f.work_package(&[("6650aaaaaaaaaaaaaaaaaaaa".into(), "jo1".into(), "itm1".into())]).unwrap();
        assert_eq!(wp.parts.len(), 1);
        assert_eq!(f.download(wp.parts[0].dxf().unwrap()).unwrap(), b"0\nEOF\n");
        let link = ErpJob { provider: Provider::Fulcrum, material_id: String::new(), material_name: String::new(), operation_id: String::new(), operation_name: String::new(), work_order: None, sent: None, sheet_item: "SHT-304-14".into() };
        let r = f.send_nest("W45 304 1/4", &link, &[("6650aaaaaaaaaaaaaaaaaaaa".into(), "jo1".into())], &report()).unwrap();
        assert!(r.created && r.error.is_none(), "{:?}", r.error);
        assert_eq!(r.work_order.as_ref().unwrap().op_id, "woop9");
        assert_eq!(r.issues, vec!["Sheet 1 (warning): Runtime looks short".to_string()]);

        let seen = seen_all.lock().unwrap();
        assert!(seen.iter().all(|s| s.1 == "authorization: Bearer tok123" || s.1 == "Authorization: Bearer tok123"), "{:?}", seen.iter().map(|s| &s.1).collect::<Vec<_>>());
        let list = seen.iter().find(|s| s.0.contains("nestable-jobs")).unwrap();
        assert!(list.0.starts_with("POST ") && list.0.contains("Take=500"));
        let lb: serde_json::Value = serde_json::from_str(&list.2).unwrap();
        assert_eq!((lb["needNesting"].as_bool(), lb["materialForms"][0].as_str(), lb["search"].as_str()), (Some(true), Some("sheet"), Some("J-1001")));
        let wo = seen.iter().find(|s| s.0 == "POST /api/work-orders").unwrap();
        let wb: serde_json::Value = serde_json::from_str(&wo.2).unwrap();
        assert_eq!((wb["status"].as_str(), wb["operations"][0]["jobOperations"][0]["jobOperationId"].as_str()), (Some("needsReview"), Some("jo1")));
        let put = seen.iter().find(|s| s.0.starts_with("PUT ")).unwrap();
        assert_eq!(put.0, "PUT /api/work-orders/6650cccccccccccccccccccc/operations/woop9/nesting-setup");
        let pb: serde_json::Value = serde_json::from_str(&put.2).unwrap();
        assert_eq!((pb["nestingSoftware"].as_str(), pb["plates"][0]["itemId"].as_str(), pb["plates"][0]["dimensionX"].as_f64()), (Some("lantek"), Some("sheet-item-id"), Some(120.0)));

        // Sending again reuses the work order: only the nest is replaced.
        let n = seen.len();
        drop(seen);
        let mut link2 = link.clone();
        link2.work_order = r.work_order.clone();
        link2.sheet_item.clear();
        let r2 = f.send_nest("W45", &link2, &[], &report()).unwrap();
        assert!(!r2.created && r2.error.is_none());
        let seen = seen_all.lock().unwrap();
        let again: Vec<&str> = seen[n..].iter().map(|s| s.0.as_str()).collect();
        assert_eq!(again, vec!["PUT /api/work-orders/6650cccccccccccccccccccc/operations/woop9/nesting-setup"]);
    }

    #[test]
    fn operations_and_work_order_steps_are_read() {
        let (url, seen) = mock(vec![
            ("/api/operations/list", 200, r#"[{"id":"op2","name":"Saw","isNestable":false,"workCenters":[],"equipment":[]},
              {"id":"op1","name":"Laser Cut","isNestable":true,"workCenters":[{"id":"wc","name":"Laser cell"}],"equipment":[{"id":"eq1","name":"Fiber 6 kW"}],"modifiedUtc":"2026-01-01T00:00:00Z"}]"#.into()),
            ("/operations/list", 200, r#"[{"id":"s2","order":2,"status":"pending","operation":{"id":"op9","name":"Brake"},"isNested":false,"jobOperations":[]},
              {"id":"s1","order":1,"status":"ready","operation":{"id":"op1","name":"Laser Cut"},"scheduledEquipment":{"id":"eq1","name":"Fiber 6 kW"},"scheduledStartUtc":"2026-11-05T07:00:00Z","isNested":true,
               "jobOperations":[{"jobId":"j","jobNumber":1001,"jobName":null,"itemToMakeId":"itm","jobOperationId":"jo1"}]}]"#.into()),
        ]);
        let f = Fulcrum::new(&url, "t".into(), Units::Inch, false, "unknown");
        let st = f.stations().unwrap();
        assert_eq!(st.iter().map(|s| (s.op.name.as_str(), s.nestable)).collect::<Vec<_>>(), vec![("Laser Cut", true), ("Saw", false)], "nestable first");
        assert_eq!((st[0].equipment[0].name.as_str(), st[0].work_centers[0].name.as_str()), ("Fiber 6 kW", "Laser cell"));
        let steps = f.work_order_steps("6650dddddddddddddddddddd").unwrap();
        assert_eq!(steps.iter().map(|s| s.id.as_str()).collect::<Vec<_>>(), vec!["s1", "s2"], "run order");
        assert_eq!((steps[0].equipment.id.as_str(), steps[0].nested), ("eq1", true));
        assert_eq!(steps[0].job_ops, vec![("j".into(), "1001".into(), "jo1".into(), "itm".into())], "the job number when it has no name");
        assert!(f.work_order_steps("../x").is_err(), "only Fulcrum ids go into the address");
        let seen = seen.lock().unwrap();
        assert!(seen[0].0.starts_with("POST /api/operations/list?") && seen[0].0.contains("Take=500"));
    }

    #[test]
    fn a_refused_token_says_so() {
        let (url, _) = mock(vec![("/api/auth/validate", 401, String::new())]);
        let f = Fulcrum::new(&url, "bad".into(), Units::Inch, false, "unknown");
        let e = f.check().unwrap_err();
        assert!(e.contains("refused the API token"), "{e}");
    }

    #[test]
    fn a_null_reference_reads_as_empty() {
        // Live answers carry `"salesOrder": null` on jobs without a sales order.
        let g: GroupJobOpDto = serde_json::from_str(r#"{"job":{"id":"j","name":"J-1"},"item":null,"salesOrder":null,"customerName":null}"#).unwrap();
        assert_eq!((g.job.to_ref().name.as_str(), g.sales_order.to_ref().name.as_str(), g.item.to_ref().id.as_str()), ("J-1", "", ""));
    }

    /// JOB-12: the connector against a live Fulcrum, reading only (sign-in,
    /// operations, waiting work, one work package and one drawing; never a
    /// nest or a work order). Skipped unless run by hand with a token:
    /// `FULCRUM_API_TOKEN=… cargo test live_read_only -- --ignored --nocapture`.
    /// Prints counts and shapes only, so its output holds no shop data.
    #[test]
    #[ignore]
    fn live_read_only() {
        let Ok(token) = std::env::var("FULCRUM_API_TOKEN") else { return };
        let base = std::env::var("FULCRUM_API_URL").unwrap_or_else(|_| DEFAULT_URL.into());
        let f = Fulcrum::new(&base, token.trim().into(), Units::Inch, false, "unknown");
        let who = f.check().expect("sign-in");
        println!("check: ok ({} chars of message)", who.len());

        let st = f.stations().expect("operations");
        println!("operations: {} ({} nestable)", st.len(), st.iter().filter(|s| s.nestable).count());

        let groups = f.groups(&GroupFilter { need_nesting: false, flat_only: true, search: String::new() }).expect("waiting work");
        let open: usize = groups.iter().map(|g| g.open.len()).sum();
        let nests: usize = groups.iter().map(|g| g.nests.len()).sum();
        println!("groups: {} (open job operations {open}, nests {nests})", groups.len());

        let Some(g) = groups.iter().find(|g| !g.open.is_empty()) else {
            println!("no open job operation to fetch a work package for");
            return;
        };
        let ops: Vec<(String, String, String)> = g.open.iter().take(3).map(|o| (o.job.id.clone(), o.op_id.clone(), o.item_to_make_id.clone())).collect();
        let wp = f.work_package(&ops).expect("work package");
        let with_mat = wp.parts.iter().filter(|p| p.material.as_ref().is_some_and(|m| m.thickness_mm.is_some())).count();
        let with_dxf = wp.parts.iter().filter(|p| p.dxf().is_some()).count();
        println!("work package: {} part(s), {with_mat} with a thickness, {with_dxf} with a DXF, {} skipped", wp.parts.len(), wp.skipped.len());

        if let Some(d) = wp.parts.iter().find_map(|p| p.dxf()) {
            let bytes = f.download(d).expect("drawing");
            let dxf_like = String::from_utf8_lossy(&bytes[..bytes.len().min(400)]).contains("SECTION");
            println!("drawing: {} bytes, looks like DXF: {dxf_like}", bytes.len());
        }
    }
}
