//! 专业分析报告生成。
//!
//! 后端负责完成全部计算并输出结构化的 Markdown 与自包含 HTML（含内联 SVG 图表），
//! 前端只负责预览、下载与打印为 PDF。

use chrono::{DateTime, SecondsFormat, Utc};
use serde::{Deserialize, Serialize};

use crate::link::{self, LinkCurvePoint, LinkOptions};
use crate::orbit::{self, AccessOptions, CoveragePoint, CoverageRegion};
use crate::radar::{self, RadarOptions};
use crate::scenario::{GroundStationObject, ScenarioData, SatelliteObject};

const EARTH_MU: f64 = 398_600.4418;
const EARTH_RADIUS_KM: f64 = 6378.1366;

fn default_step() -> f64 {
    60.0
}

fn default_grid() -> f64 {
    5.0
}

fn default_min_elevation() -> f64 {
    5.0
}

fn default_true() -> bool {
    true
}

/// 报告生成请求。
#[derive(Debug, Clone, Deserialize)]
pub struct ReportRequest {
    #[serde(default)]
    pub title: Option<String>,
    pub start: String,
    pub end: String,
    #[serde(default = "default_step")]
    pub step_seconds: f64,
    #[serde(default)]
    pub satellite_ids: Option<Vec<String>>,
    #[serde(default)]
    pub station_ids: Option<Vec<String>>,
    #[serde(default)]
    pub region: Option<CoverageRegion>,
    #[serde(default = "default_grid")]
    pub grid_step_deg: f64,
    #[serde(default = "default_min_elevation")]
    pub min_elevation_deg: f64,
    #[serde(default = "default_true")]
    pub include_access: bool,
    #[serde(default = "default_true")]
    pub include_coverage: bool,
    #[serde(default = "default_true")]
    pub include_link: bool,
    #[serde(default = "default_true")]
    pub include_radar: bool,
    #[serde(default)]
    pub link_options: Option<LinkOptions>,
    #[serde(default)]
    pub radar_options: Option<RadarOptions>,
}

#[derive(Debug, Clone, Serialize)]
pub struct AccessSummary {
    pub satellite_name: String,
    pub station_name: String,
    pub window_count: u32,
    pub total_duration_seconds: f64,
    pub longest_window_seconds: f64,
    pub mean_gap_seconds: f64,
    pub max_gap_seconds: f64,
    pub windows: Vec<(f64, f64)>,
}

#[derive(Debug, Clone, Serialize)]
pub struct CoverageSummary {
    pub coverage_ratio: f64,
    pub covered_points: usize,
    pub total_points: usize,
    pub mean_revisit_seconds: f64,
    pub max_gap_seconds: f64,
    pub average_visible_seconds: f64,
    pub region: CoverageRegion,
    pub grid_step_deg: f64,
    pub points: Vec<CoveragePoint>,
}

#[derive(Debug, Clone, Serialize)]
pub struct LinkSummary {
    pub satellite_name: String,
    pub station_name: String,
    pub window_count: u32,
    pub min_margin_db: f64,
    pub mean_margin_db: f64,
    pub mean_atmospheric_loss_db: f64,
    pub max_range_km: f64,
    pub curve: Vec<LinkCurvePoint>,
    pub required_cn0_dbhz: f64,
}

#[derive(Debug, Clone, Serialize)]
pub struct RadarSummary {
    pub satellite_name: String,
    pub station_name: String,
    pub max_detection_range_km: f64,
    pub detected_window_count: u32,
    pub total_detection_seconds: f64,
    pub required_snr_db: f64,
    pub curve: Vec<(f64, f64)>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReportStats {
    pub satellite_count: usize,
    pub station_count: usize,
    pub access_pair_count: usize,
    pub access_window_count: u32,
    pub coverage_ratio: Option<f64>,
    pub max_gap_seconds: Option<f64>,
    pub min_link_margin_db: Option<f64>,
    pub radar_max_detection_range_km: Option<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReportDocument {
    pub title: String,
    pub generated_at: String,
    pub start: String,
    pub end: String,
    pub stats: ReportStats,
    pub markdown: String,
    pub html: String,
}

fn format_utc(time: DateTime<Utc>) -> String {
    time.to_rfc3339_opts(SecondsFormat::Millis, true)
}

fn escape_html(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

fn format_number(value: f64, digits: usize) -> String {
    format!("{value:.digits$}")
}

fn orbital_summary(satellite: &SatelliteObject) -> (f64, f64, f64) {
    let elements = sgp4::Elements::from_tle(
        None,
        satellite.tle_line1.trim().as_bytes(),
        satellite.tle_line2.trim().as_bytes(),
    );
    match elements {
        Ok(elements) => {
            let mean_motion_rad_s = elements.mean_motion / 60.0;
            let semi_major_axis = if mean_motion_rad_s > 0.0 {
                (EARTH_MU / mean_motion_rad_s.powi(2)).cbrt()
            } else {
                EARTH_RADIUS_KM
            };
            let period_minutes = if elements.mean_motion > 0.0 {
                1440.0 / elements.mean_motion
            } else {
                0.0
            };
            (
                semi_major_axis - EARTH_RADIUS_KM,
                elements.inclination.to_degrees(),
                period_minutes,
            )
        }
        Err(_) => (0.0, 0.0, 0.0),
    }
}

fn access_gap_statistics(windows: &[(f64, f64)], total_seconds: f64) -> (f64, f64) {
    if windows.len() >= 2 {
        let mut sum = 0.0_f64;
        let mut maximum = 0.0_f64;
        for pair in windows.windows(2) {
            let gap = (pair[1].0 - pair[0].1).max(0.0);
            sum += gap;
            maximum = maximum.max(gap);
        }
        (sum / (windows.len() - 1) as f64, maximum)
    } else if windows.len() == 1 {
        let gap = (total_seconds - (windows[0].1 - windows[0].0)).max(0.0);
        (gap, gap)
    } else {
        (total_seconds, total_seconds)
    }
}

fn select_satellites<'a>(
    data: &'a ScenarioData,
    ids: Option<&[String]>,
) -> Vec<&'a SatelliteObject> {
    data.satellites
        .iter()
        .filter(|satellite| match ids {
            Some(ids) => ids.iter().any(|id| id == &satellite.id),
            None => satellite.enabled,
        })
        .take(10)
        .collect()
}

fn select_stations<'a>(
    data: &'a ScenarioData,
    ids: Option<&[String]>,
) -> Vec<&'a GroundStationObject> {
    data.ground_stations
        .iter()
        .filter(|station| match ids {
            Some(ids) => ids.iter().any(|id| id == &station.id),
            None => station.enabled,
        })
        .take(5)
        .collect()
}

fn svg_line_chart(
    points: &[(f64, f64)],
    x_max: f64,
    y_label: &str,
    color: &str,
    threshold: Option<f64>,
) -> String {
    let width = 920.0_f64;
    let height = 220.0_f64;
    let left = 64.0_f64;
    let top = 16.0_f64;
    let bottom = 34.0_f64;
    let plot_width = width - left - 20.0;
    let plot_height = height - top - bottom;
    if points.is_empty() {
        return "<p class=\"muted\">无曲线数据</p>".to_string();
    }
    let mut min_y = points
        .iter()
        .map(|(_, y)| *y)
        .fold(f64::INFINITY, f64::min);
    let mut max_y = points
        .iter()
        .map(|(_, y)| *y)
        .fold(f64::NEG_INFINITY, f64::max);
    if let Some(value) = threshold {
        min_y = min_y.min(value);
        max_y = max_y.max(value);
    }
    if (max_y - min_y).abs() < 1e-6 {
        min_y -= 1.0;
        max_y += 1.0;
    }
    let padding = (max_y - min_y) * 0.08;
    let min_y = min_y - padding;
    let max_y = max_y + padding;
    let scale_x = |x: f64| left + (x / x_max.max(1e-6)).clamp(0.0, 1.0) * plot_width;
    let scale_y = |y: f64| top + (1.0 - (y - min_y) / (max_y - min_y)) * plot_height;

    let mut path = String::new();
    for (index, (x, y)) in points.iter().enumerate() {
        let command = if index == 0 { 'M' } else { 'L' };
        path.push_str(&format!(
            "{command}{:.1},{:.1} ",
            scale_x(*x),
            scale_y(*y)
        ));
    }

    let mut threshold_line = String::new();
    if let Some(value) = threshold {
        let y = scale_y(value);
        threshold_line = format!(
            "<line x1=\"{left:.1}\" y1=\"{y:.1}\" x2=\"{:.1}\" y2=\"{y:.1}\" stroke=\"#ff8a65\" stroke-dasharray=\"5 4\" stroke-width=\"1\"/>",
            left + plot_width
        );
    }

    format!(
        "<svg viewBox=\"0 0 {width:.0} {height:.0}\" class=\"chart\" role=\"img\">\
<rect x=\"0\" y=\"0\" width=\"{width:.0}\" height=\"{height:.0}\" fill=\"transparent\"/>\
<line x1=\"{left:.1}\" y1=\"{top:.1}\" x2=\"{left:.1}\" y2=\"{:.1}\" stroke=\"#37474f\"/>\
<line x1=\"{left:.1}\" y1=\"{:.1}\" x2=\"{:.1}\" y2=\"{:.1}\" stroke=\"#37474f\"/>\
<text x=\"8\" y=\"{:.1}\" fill=\"#90a4ae\" font-size=\"11\">{:.1}</text>\
<text x=\"8\" y=\"{:.1}\" fill=\"#90a4ae\" font-size=\"11\">{:.1}</text>\
<text x=\"{:.1}\" y=\"{:.1}\" fill=\"#90a4ae\" font-size=\"11\">{y_label}</text>\
{threshold_line}\
<path d=\"{path}\" fill=\"none\" stroke=\"{color}\" stroke-width=\"1.8\"/>\
</svg>",
        top + plot_height,
        top,
        left,
        top + plot_height,
        scale_y(max_y),
        max_y,
        scale_y(min_y),
        min_y,
        left + plot_width / 2.0,
        height - 8.0,
    )
}

fn svg_access_timeline(summaries: &[AccessSummary], total_seconds: f64) -> String {
    let rows = summaries.len();
    if rows == 0 {
        return "<p class=\"muted\">无 Access 窗口</p>".to_string();
    }
    let row_height = 28.0_f64;
    let label_width = 210.0_f64;
    let width = 920.0_f64;
    let top = 16.0_f64;
    let height = top * 2.0 + row_height * rows as f64;
    let plot_width = width - label_width - 20.0;

    let mut content = String::new();
    for (index, summary) in summaries.iter().enumerate() {
        let y = top + index as f64 * row_height;
        content.push_str(&format!(
            "<text x=\"4\" y=\"{:.1}\" fill=\"#cfd8dc\" font-size=\"11\">{} / {}</text>",
            y + 15.0,
            escape_html(&truncate(&summary.satellite_name, 16)),
            escape_html(&truncate(&summary.station_name, 10))
        ));
        for (start, end) in &summary.windows {
            let x = label_width + (start / total_seconds.max(1e-6)).clamp(0.0, 1.0) * plot_width;
            let width_value =
                ((end - start) / total_seconds.max(1e-6)).clamp(0.0, 1.0) * plot_width;
            content.push_str(&format!(
                "<rect x=\"{x:.1}\" y=\"{:.1}\" width=\"{:.1}\" height=\"14\" rx=\"2\" fill=\"#4fc3f7\" opacity=\"0.85\"/>",
                y + 3.0,
                width_value.max(1.5)
            ));
        }
    }

    format!(
        "<svg viewBox=\"0 0 {width:.0} {height:.0}\" class=\"chart\">\
<line x1=\"{label_width:.1}\" y1=\"{top:.1}\" x2=\"{label_width:.1}\" y2=\"{:.1}\" stroke=\"#37474f\"/>\
<line x1=\"{label_width:.1}\" y1=\"{:.1}\" x2=\"{:.1}\" y2=\"{:.1}\" stroke=\"#37474f\"/>\
<text x=\"4\" y=\"{:.1}\" fill=\"#90a4ae\" font-size=\"11\">Access 窗口时间线</text>\
{content}\
</svg>",
        top + row_height * rows as f64,
        top + row_height * rows as f64,
        width - 20.0,
        top + row_height * rows as f64,
        height - 2.0,
    )
}

fn svg_coverage_heatmap(summary: &CoverageSummary) -> String {
    let latitude_steps =
        ((summary.region.max_latitude_deg - summary.region.min_latitude_deg)
            / summary.grid_step_deg)
            .ceil()
            .max(1.0) as usize;
    let longitude_steps =
        ((summary.region.max_longitude_deg - summary.region.min_longitude_deg)
            / summary.grid_step_deg)
            .ceil()
            .max(1.0) as usize;
    let width = 920.0_f64;
    let height = 300.0_f64;
    let left = 46.0_f64;
    let top = 18.0_f64;
    let bottom = 30.0_f64;
    let plot_width = width - left - 24.0;
    let plot_height = height - top - bottom;
    let cell_width = plot_width / (longitude_steps + 1) as f64;
    let cell_height = plot_height / (latitude_steps + 1) as f64;

    let mut cells = String::new();
    for point in &summary.points {
        let x = left
            + (point.longitude_deg - summary.region.min_longitude_deg)
                / (summary.region.max_longitude_deg - summary.region.min_longitude_deg)
                    .max(1e-6)
                * plot_width;
        let y = top
            + (1.0
                - (point.latitude_deg - summary.region.min_latitude_deg)
                    / (summary.region.max_latitude_deg - summary.region.min_latitude_deg)
                        .max(1e-6))
                * plot_height;
        let intensity = if point.covered {
            0.35 + (point.visible_seconds / (summary.average_visible_seconds.max(1.0))).min(1.0) * 0.6
        } else {
            0.0
        };
        let fill = if point.covered { "#4fc3f7" } else { "#263238" };
        cells.push_str(&format!(
            "<rect x=\"{x:.1}\" y=\"{y:.1}\" width=\"{:.1}\" height=\"{:.1}\" fill=\"{fill}\" opacity=\"{intensity:.2}\"/>",
            cell_width.max(1.0),
            cell_height.max(1.0)
        ));
    }
    format!(
        "<svg viewBox=\"0 0 {width:.0} {height:.0}\" class=\"chart\">\
<text x=\"4\" y=\"12\" fill=\"#90a4ae\" font-size=\"11\">覆盖热力图（纬 {:.1}°~{:.1}°，经 {:.1}°~{:.1}°）</text>\
{cells}\
<rect x=\"{left:.1}\" y=\"{top:.1}\" width=\"{plot_width:.1}\" height=\"{plot_height:.1}\" fill=\"none\" stroke=\"#37474f\"/>\
</svg>",
        summary.region.min_latitude_deg,
        summary.region.max_latitude_deg,
        summary.region.min_longitude_deg,
        summary.region.max_longitude_deg,
    )
}

fn truncate(value: &str, length: usize) -> String {
    let chars: Vec<char> = value.chars().collect();
    if chars.len() <= length {
        value.to_string()
    } else {
        format!("{}…", chars[..length].iter().collect::<String>())
    }
}

/// 生成报告文档。
pub fn generate(data: &ScenarioData, request: &ReportRequest) -> Result<ReportDocument, String> {
    let start = DateTime::parse_from_rfc3339(request.start.trim())
        .map_err(|_| "开始时间需为 RFC3339 格式".to_string())?
        .with_timezone(&Utc);
    let end = DateTime::parse_from_rfc3339(request.end.trim())
        .map_err(|_| "结束时间需为 RFC3339 格式".to_string())?
        .with_timezone(&Utc);
    if end <= start {
        return Err("结束时间必须晚于开始时间".into());
    }
    if !request.step_seconds.is_finite() || !(1.0..=3_600.0).contains(&request.step_seconds) {
        return Err("报告采样步长需在 [1, 3600] 秒内".into());
    }
    if !request.min_elevation_deg.is_finite() || !(0.0..=90.0).contains(&request.min_elevation_deg) {
        return Err("最小仰角需在 [0, 90] 度内".into());
    }

    let total_seconds = (end - start).num_milliseconds() as f64 / 1000.0;
    let satellites = select_satellites(data, request.satellite_ids.as_deref());
    if satellites.is_empty() {
        return Err("报告至少需要一颗启用的卫星".into());
    }
    let stations = select_stations(data, request.station_ids.as_deref());

    // 1. Access 汇总（最多 8 对，避免报告生成过慢）。
    let mut access_summaries: Vec<AccessSummary> = Vec::new();
    if request.include_access {
        'outer: for satellite in &satellites {
            for station in &stations {
                if access_summaries.len() >= 8 {
                    break 'outer;
                }
                let min_elevation = request.min_elevation_deg.max(station.min_elevation_deg);
                let options = AccessOptions {
                    min_elevation_deg: Some(min_elevation),
                    ..Default::default()
                };
                let report = match orbit::compute_access(
                    data,
                    &satellite.id,
                    &station.id,
                    start,
                    end,
                    request.step_seconds,
                    &options,
                ) {
                    Ok(report) => report,
                    Err(_) => continue,
                };
                let windows: Vec<(f64, f64)> = report
                    .windows
                    .iter()
                    .map(|window| (window.start_seconds, window.end_seconds))
                    .collect();
                let (mean_gap, max_gap) = access_gap_statistics(&windows, total_seconds);
                access_summaries.push(AccessSummary {
                    satellite_name: satellite.name.clone(),
                    station_name: station.name.clone(),
                    window_count: report.window_count,
                    total_duration_seconds: report.total_duration_seconds,
                    longest_window_seconds: report.longest_window_seconds,
                    mean_gap_seconds: mean_gap,
                    max_gap_seconds: max_gap,
                    windows,
                });
            }
        }
    }

    // 2. 覆盖汇总。
    let mut coverage_summary: Option<CoverageSummary> = None;
    if request.include_coverage {
        if let Some(region) = request.region {
            let analysis = orbit::compute_constellation_analysis(
                data,
                request.satellite_ids.as_deref(),
                request.station_ids.as_deref(),
                start,
                end,
                request.step_seconds.max(10.0),
                region,
                request.grid_step_deg,
                request.min_elevation_deg,
            )?;
            coverage_summary = Some(CoverageSummary {
                coverage_ratio: analysis.coverage_ratio,
                covered_points: analysis.covered_points,
                total_points: analysis.total_points,
                mean_revisit_seconds: analysis.mean_revisit_seconds,
                max_gap_seconds: analysis.max_gap_seconds,
                average_visible_seconds: analysis.coverage.average_visible_seconds,
                region,
                grid_step_deg: request.grid_step_deg,
                points: analysis.coverage.points.clone(),
            });
        }
    }

    // 3. 链路汇总（第一对卫星-地面站，使用卫星对象默认参数作为回退）。
    let mut link_summary: Option<LinkSummary> = None;
    if request.include_link {
        if let (Some(satellite), Some(station)) = (satellites.first(), stations.first()) {
            let options = request
                .link_options
                .clone()
                .or_else(|| satellite.link_options.clone())
                .unwrap_or_default();
            if let Ok(report) = link::compute_link_budget(
                data,
                &satellite.id,
                &station.id,
                start,
                end,
                request.step_seconds,
                &options,
            ) {
                link_summary = Some(LinkSummary {
                    satellite_name: satellite.name.clone(),
                    station_name: station.name.clone(),
                    window_count: report.window_count,
                    min_margin_db: report.min_margin_db,
                    mean_margin_db: report.mean_margin_db,
                    mean_atmospheric_loss_db: report.mean_atmospheric_loss_db,
                    max_range_km: report.max_range_km,
                    curve: report.curve,
                    required_cn0_dbhz: options.required_cn0_dbhz,
                });
            }
        }
    }

    // 4. 雷达汇总。
    let mut radar_summary: Option<RadarSummary> = None;
    if request.include_radar {
        if let (Some(satellite), Some(station)) = (satellites.first(), stations.first()) {
            let options = request
                .radar_options
                .clone()
                .or_else(|| satellite.radar_options.clone())
                .unwrap_or_default();
            if let Ok(report) = radar::compute_radar(
                data,
                &satellite.id,
                &station.id,
                start,
                end,
                request.step_seconds,
                &options,
            ) {
                radar_summary = Some(RadarSummary {
                    satellite_name: satellite.name.clone(),
                    station_name: station.name.clone(),
                    max_detection_range_km: report.max_detection_range_km,
                    detected_window_count: report.detected_window_count,
                    total_detection_seconds: report.total_detection_seconds,
                    required_snr_db: report.required_snr_db,
                    curve: report
                        .samples
                        .iter()
                        .map(|sample| (sample.t_seconds, sample.snr_db))
                        .collect(),
                });
            }
        }
    }

    let title = request
        .title
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or("OSKT 场景分析报告")
        .to_string();
    let generated_at = format_utc(Utc::now());
    let start_text = format_utc(start);
    let end_text = format_utc(end);

    let stats = ReportStats {
        satellite_count: satellites.len(),
        station_count: stations.len(),
        access_pair_count: access_summaries.len(),
        access_window_count: access_summaries.iter().map(|item| item.window_count).sum(),
        coverage_ratio: coverage_summary.as_ref().map(|item| item.coverage_ratio),
        max_gap_seconds: coverage_summary.as_ref().map(|item| item.max_gap_seconds),
        min_link_margin_db: link_summary.as_ref().map(|item| item.min_margin_db),
        radar_max_detection_range_km: radar_summary
            .as_ref()
            .map(|item| item.max_detection_range_km),
    };

    // ---------- Markdown ----------
    let mut markdown = String::new();
    markdown.push_str(&format!("# {title}\n\n"));
    markdown.push_str(&format!(
        "> 生成时间：{generated_at}  \n> 分析区间：{start_text} ~ {end_text}  \n> 采样步长：{} s\n\n",
        request.step_seconds
    ));
    markdown.push_str("## 1. 场景摘要\n\n");
    markdown.push_str(&format!(
        "- 参与卫星：{} 颗\n- 地面站：{} 个\n- Access 对比对：{} 对\n\n",
        satellites.len(),
        stations.len(),
        access_summaries.len()
    ));
    markdown.push_str("| 卫星 | 近圆轨道高度 (km) | 倾角 (°) | 周期 (min) |\n| --- | ---: | ---: | ---: |\n");
    for satellite in &satellites {
        let (altitude, inclination, period) = orbital_summary(satellite);
        markdown.push_str(&format!(
            "| {} | {} | {} | {} |\n",
            satellite.name,
            format_number(altitude, 1),
            format_number(inclination, 2),
            format_number(period, 2)
        ));
    }
    markdown.push('\n');
    if !stations.is_empty() {
        markdown.push_str("| 地面站 | 纬度 (°) | 经度 (°) | 最小仰角 (°) |\n| --- | ---: | ---: | ---: |\n");
        for station in &stations {
            markdown.push_str(&format!(
                "| {} | {} | {} | {} |\n",
                station.name,
                format_number(station.latitude_deg, 3),
                format_number(station.longitude_deg, 3),
                format_number(station.min_elevation_deg, 1)
            ));
        }
        markdown.push('\n');
    }

    if !access_summaries.is_empty() {
        markdown.push_str("## 2. Access 统计\n\n");
        markdown.push_str("| 卫星 | 地面站 | 窗口数 | 总时长 (s) | 最长窗口 (s) | 平均间隙 (s) | 最大间隙 (s) |\n| --- | --- | ---: | ---: | ---: | ---: | ---: |\n");
        for summary in &access_summaries {
            markdown.push_str(&format!(
                "| {} | {} | {} | {} | {} | {} | {} |\n",
                summary.satellite_name,
                summary.station_name,
                summary.window_count,
                format_number(summary.total_duration_seconds, 1),
                format_number(summary.longest_window_seconds, 1),
                format_number(summary.mean_gap_seconds, 1),
                format_number(summary.max_gap_seconds, 1)
            ));
        }
        markdown.push_str("\n![Access 窗口时间线](access-timeline)\n\n");
    }

    if let Some(summary) = &coverage_summary {
        markdown.push_str("## 3. 覆盖统计\n\n");
        markdown.push_str(&format!(
            "- 覆盖率：{}%（{}/{} 个网格点）\n- 平均重访：{} s\n- 最大间隙：{} s\n- 平均可见时长：{} s\n- 区域：纬 {}°~{}°，经 {}°~{}°，网格 {}°\n\n",
            format_number(summary.coverage_ratio * 100.0, 2),
            summary.covered_points,
            summary.total_points,
            format_number(summary.mean_revisit_seconds, 1),
            format_number(summary.max_gap_seconds, 1),
            format_number(summary.average_visible_seconds, 1),
            format_number(summary.region.min_latitude_deg, 1),
            format_number(summary.region.max_latitude_deg, 1),
            format_number(summary.region.min_longitude_deg, 1),
            format_number(summary.region.max_longitude_deg, 1),
            format_number(summary.grid_step_deg, 1)
        ));
        markdown.push_str("![覆盖热力图](coverage-heatmap)\n\n");
    }

    if let Some(summary) = &link_summary {
        markdown.push_str("## 4. 链路预算\n\n");
        markdown.push_str("| 卫星 | 地面站 | 窗口数 | 最小余量 (dB) | 平均余量 (dB) | 平均大气损耗 (dB) | 最大斜距 (km) |\n| --- | --- | ---: | ---: | ---: | ---: | ---: |\n");
        markdown.push_str(&format!(
            "| {} | {} | {} | {} | {} | {} | {} |\n\n",
            summary.satellite_name,
            summary.station_name,
            summary.window_count,
            format_number(summary.min_margin_db, 2),
            format_number(summary.mean_margin_db, 2),
            format_number(summary.mean_atmospheric_loss_db, 3),
            format_number(summary.max_range_km, 1)
        ));
        markdown.push_str(&format!(
            "> 链路余量相对所需 C/N0 = {} dB-Hz 计算。\n\n",
            summary.required_cn0_dbhz
        ));
        markdown.push_str("![链路余量曲线](link-margin)\n\n");
    }

    if let Some(summary) = &radar_summary {
        markdown.push_str("## 5. 雷达探测\n\n");
        markdown.push_str(&format!(
            "- 最大探测距离：{} km\n- 探测窗口：{} 个\n- 累计探测时长：{} s\n- 检测门限 SNR：{} dB\n\n",
            format_number(summary.max_detection_range_km, 1),
            summary.detected_window_count,
            format_number(summary.total_detection_seconds, 1),
            format_number(summary.required_snr_db, 1)
        ));
        markdown.push_str("![雷达 SNR 曲线](radar-snr)\n\n");
    }

    markdown.push_str("## 6. 说明\n\n");
    markdown.push_str("- 本报告所有轨道、Access、覆盖、链路与雷达计算均由 Rust 后端完成。\n");
    markdown.push_str("- 覆盖统计中，平均重访为同一网格点相邻 Access 窗口间隙的均值；最大间隙为分析窗口内最坏值。\n");
    markdown.push_str("- Markdown 中的图表占位说明请使用 HTML 版本查看内联 SVG 图表。\n");

    // ---------- HTML ----------
    let access_timeline = svg_access_timeline(&access_summaries, total_seconds);
    let coverage_chart = coverage_summary
        .as_ref()
        .map(svg_coverage_heatmap)
        .unwrap_or_default();
    let link_chart = link_summary
        .as_ref()
        .map(|summary| {
            svg_line_chart(
                &summary
                    .curve
                    .iter()
                    .map(|point| (point.t_seconds, point.margin_db))
                    .collect::<Vec<_>>(),
                total_seconds,
                "链路余量 (dB)",
                "#81c784",
                Some(0.0),
            )
        })
        .unwrap_or_default();
    let radar_chart = radar_summary
        .as_ref()
        .map(|summary| {
            svg_line_chart(
                &summary.curve,
                total_seconds,
                "SNR (dB)",
                "#ffb74d",
                Some(summary.required_snr_db),
            )
        })
        .unwrap_or_default();

    let mut html = String::new();
    html.push_str("<!DOCTYPE html><html lang=\"zh-CN\"><head><meta charset=\"utf-8\">");
    html.push_str(&format!("<title>{}</title>", escape_html(&title)));
    html.push_str("<style>");
    html.push_str(
        "body{margin:0;background:#0f1a20;color:#e0e6ea;font-family:'Segoe UI','Microsoft YaHei',sans-serif;line-height:1.65;}",
    );
    html.push_str(".page{max-width:1000px;margin:0 auto;padding:40px 32px 64px;}");
    html.push_str("h1{font-size:30px;margin:0 0 12px;color:#fff;}h2{font-size:21px;margin:36px 0 12px;color:#4fc3f7;border-left:3px solid #4fc3f7;padding-left:10px;}");
    html.push_str(".meta{color:#90a4ae;font-size:13px;margin-bottom:24px;}");
    html.push_str("table{width:100%;border-collapse:collapse;margin:14px 0;font-size:13px;}");
    html.push_str("th,td{border:1px solid #2b3b43;padding:7px 9px;text-align:left;}th{background:#16252c;color:#b0bec5;}td{background:#101d23;}");
    html.push_str(".cards{display:grid;grid-template-columns:repeat(auto-fit,minmax(180px,1fr));gap:12px;margin:18px 0;}");
    html.push_str(".card{background:#132028;border:1px solid #24343c;border-radius:10px;padding:14px 16px;}");
    html.push_str(".card .label{font-size:12px;color:#90a4ae;}.card .value{font-size:20px;color:#4fc3f7;margin-top:4px;}");
    html.push_str(".chart{width:100%;height:auto;background:#0c171d;border:1px solid #1f2f37;border-radius:8px;margin:10px 0;}");
    html.push_str(".muted{color:#78909c;}.note{color:#90a4ae;font-size:13px;}");
    html.push_str("@media print{body{background:#fff;color:#111}.page{max-width:none}h2{color:#0277bd}.card,td{background:#fff}table{border-color:#ccc}}");
    html.push_str("</style></head><body><div class=\"page\">");
    html.push_str(&format!("<h1>{}</h1>", escape_html(&title)));
    html.push_str(&format!(
        "<div class=\"meta\">生成时间：{} ｜ 分析区间：{} ~ {} ｜ 步长：{} s</div>",
        escape_html(&generated_at),
        escape_html(&start_text),
        escape_html(&end_text),
        request.step_seconds
    ));
    html.push_str("<div class=\"cards\">");
    html.push_str(&format!(
        "<div class=\"card\"><div class=\"label\">卫星</div><div class=\"value\">{}</div></div>",
        satellites.len()
    ));
    html.push_str(&format!(
        "<div class=\"card\"><div class=\"label\">地面站</div><div class=\"value\">{}</div></div>",
        stations.len()
    ));
    html.push_str(&format!(
        "<div class=\"card\"><div class=\"label\">Access 窗口</div><div class=\"value\">{}</div></div>",
        stats.access_window_count
    ));
    if let Some(ratio) = stats.coverage_ratio {
        html.push_str(&format!(
            "<div class=\"card\"><div class=\"label\">覆盖率</div><div class=\"value\">{}%</div></div>",
            format_number(ratio * 100.0, 1)
        ));
    }
    if let Some(max_gap) = stats.max_gap_seconds {
        html.push_str(&format!(
            "<div class=\"card\"><div class=\"label\">最大间隙</div><div class=\"value\">{} s</div></div>",
            format_number(max_gap, 0)
        ));
    }
    if let Some(margin) = stats.min_link_margin_db {
        html.push_str(&format!(
            "<div class=\"card\"><div class=\"label\">最小链路余量</div><div class=\"value\">{} dB</div></div>",
            format_number(margin, 2)
        ));
    }
    if let Some(range) = stats.radar_max_detection_range_km {
        html.push_str(&format!(
            "<div class=\"card\"><div class=\"label\">雷达最大探测距离</div><div class=\"value\">{} km</div></div>",
            format_number(range, 1)
        ));
    }
    html.push_str("</div>");

    html.push_str("<h2>1. 场景摘要</h2><table><thead><tr><th>卫星</th><th>高度 (km)</th><th>倾角 (°)</th><th>周期 (min)</th></tr></thead><tbody>");
    for satellite in &satellites {
        let (altitude, inclination, period) = orbital_summary(satellite);
        html.push_str(&format!(
            "<tr><td>{}</td><td>{}</td><td>{}</td><td>{}</td></tr>",
            escape_html(&satellite.name),
            format_number(altitude, 1),
            format_number(inclination, 2),
            format_number(period, 2)
        ));
    }
    html.push_str("</tbody></table>");

    if !access_summaries.is_empty() {
        html.push_str("<h2>2. Access 统计</h2>");
        html.push_str("<table><thead><tr><th>卫星</th><th>地面站</th><th>窗口数</th><th>总时长 (s)</th><th>最长窗口 (s)</th><th>平均间隙 (s)</th><th>最大间隙 (s)</th></tr></thead><tbody>");
        for summary in &access_summaries {
            html.push_str(&format!(
                "<tr><td>{}</td><td>{}</td><td>{}</td><td>{}</td><td>{}</td><td>{}</td><td>{}</td></tr>",
                escape_html(&summary.satellite_name),
                escape_html(&summary.station_name),
                summary.window_count,
                format_number(summary.total_duration_seconds, 1),
                format_number(summary.longest_window_seconds, 1),
                format_number(summary.mean_gap_seconds, 1),
                format_number(summary.max_gap_seconds, 1)
            ));
        }
        html.push_str("</tbody></table>");
        html.push_str(&access_timeline);
    }

    if let Some(summary) = &coverage_summary {
        html.push_str("<h2>3. 覆盖统计</h2>");
        html.push_str(&format!(
            "<p>覆盖率 <strong>{}%</strong>（{}/{} 点），平均重访 {} s，最大间隙 {} s，平均可见 {} s。</p>",
            format_number(summary.coverage_ratio * 100.0, 2),
            summary.covered_points,
            summary.total_points,
            format_number(summary.mean_revisit_seconds, 1),
            format_number(summary.max_gap_seconds, 1),
            format_number(summary.average_visible_seconds, 1)
        ));
        html.push_str(&coverage_chart);
    }

    if let Some(summary) = &link_summary {
        html.push_str("<h2>4. 链路预算</h2>");
        html.push_str("<table><thead><tr><th>卫星</th><th>地面站</th><th>窗口数</th><th>最小余量 (dB)</th><th>平均余量 (dB)</th><th>平均大气损耗 (dB)</th><th>最大斜距 (km)</th></tr></thead><tbody>");
        html.push_str(&format!(
            "<tr><td>{}</td><td>{}</td><td>{}</td><td>{}</td><td>{}</td><td>{}</td><td>{}</td></tr>",
            escape_html(&summary.satellite_name),
            escape_html(&summary.station_name),
            summary.window_count,
            format_number(summary.min_margin_db, 2),
            format_number(summary.mean_margin_db, 2),
            format_number(summary.mean_atmospheric_loss_db, 3),
            format_number(summary.max_range_km, 1)
        ));
        html.push_str("</tbody></table>");
        html.push_str(&link_chart);
    }

    if let Some(summary) = &radar_summary {
        html.push_str("<h2>5. 雷达探测</h2>");
        html.push_str(&format!(
            "<p>最大探测距离 <strong>{} km</strong>，探测窗口 {} 个，累计探测 {} s，检测门限 {} dB。</p>",
            format_number(summary.max_detection_range_km, 1),
            summary.detected_window_count,
            format_number(summary.total_detection_seconds, 1),
            format_number(summary.required_snr_db, 1)
        ));
        html.push_str(&radar_chart);
    }

    html.push_str("<h2>6. 说明</h2><p class=\"note\">本报告全部核心计算由 Rust 后端完成；覆盖统计的平均重访为同一网格点相邻 Access 窗口间隙均值，最大间隙为分析窗口内最坏值。可使用浏览器“打印 → 另存为 PDF”导出 PDF。</p>");
    html.push_str("</div></body></html>");

    Ok(ReportDocument {
        title,
        generated_at,
        start: start_text,
        end: end_text,
        stats,
        markdown,
        html,
    })
}
