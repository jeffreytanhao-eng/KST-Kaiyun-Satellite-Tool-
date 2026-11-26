#!/usr/bin/env python3
"""KST Phase 4 API 自动化示例。

演示完整分析流程：
1. 登录并创建场景（含地面站）
2. 生成 Walker Delta 星座并写回场景
3. 依次执行传播、Access、星座批量分析、链路预算、雷达探测、同频干扰与报告
4. 轮询异步任务并导出 CSV / HTML 报告

仅依赖 Python 3 标准库。用法：
  python kst_api_demo.py --base-url http://127.0.0.1:18080 --out ./kst_demo_out
"""

from __future__ import annotations

import argparse
import datetime as dt
import json
import os
import sys
import time
import urllib.error
import urllib.request
from typing import Any

API_PREFIX = "/api/v1"


class KstClient:
    def __init__(self, base_url: str, username: str, password: str) -> None:
        self.base_url = base_url.rstrip("/")
        self.username = username
        self.password = password
        self.token: str | None = None
        self.out_dir = "."
        self.last_job_id: str | None = None

    def _request(
        self,
        method: str,
        path: str,
        payload: dict[str, Any] | None = None,
        raw: bool = False,
    ) -> Any:
        url = f"{self.base_url}{API_PREFIX}{path}"
        body = None
        headers = {"Accept": "application/json"}
        if payload is not None:
            body = json.dumps(payload).encode("utf-8")
            headers["Content-Type"] = "application/json"
        if self.token:
            headers["Authorization"] = f"Bearer {self.token}"
        request = urllib.request.Request(url, data=body, headers=headers, method=method)
        try:
            with urllib.request.urlopen(request, timeout=60) as response:
                data = response.read()
                if raw:
                    return data
                return json.loads(data.decode("utf-8")) if data else None
        except urllib.error.HTTPError as error:
            detail = error.read().decode("utf-8", errors="replace")
            raise RuntimeError(f"{method} {path} -> HTTP {error.code}: {detail}") from error

    def login(self) -> None:
        result = self._request(
            "POST",
            "/auth/login",
            {"username": self.username, "password": self.password},
        )
        self.token = result["token"]
        print(f"[login] {self.username} 登录成功")

    def create_scenario(self, name: str, data: dict[str, Any]) -> dict[str, Any]:
        scenario = self._request(
            "POST",
            "/scenarios",
            {"name": name, "description": "Phase 4 API 自动化示例场景", "data": data},
        )
        print(f"[scenario] 已创建 {scenario['id']}")
        return scenario

    def update_scenario(self, scenario_id: str, name: str, data: dict[str, Any]) -> None:
        self._request(
            "PUT",
            f"/scenarios/{scenario_id}",
            {"name": name, "description": "Phase 4 API 自动化示例场景", "data": data},
        )

    def delete_scenario(self, scenario_id: str) -> None:
        self._request("DELETE", f"/scenarios/{scenario_id}")
        print(f"[scenario] 已删除 {scenario_id}")

    def generate_constellation(self, payload: dict[str, Any]) -> dict[str, Any]:
        report = self._request("POST", "/constellations/generate", payload)
        print(
            f"[constellation] {report['constellation_type']} "
            f"{report['total_satellites']} 星 / {report['planes']} 面"
        )
        return report

    def run_job(self, scenario_id: str, kind: str, params: dict[str, Any]) -> dict[str, Any]:
        job = self._request(
            "POST",
            "/jobs",
            {"scenario_id": scenario_id, "kind": kind, "params": params},
        )
        job_id = job["id"]
        self.last_job_id = job_id
        started = time.time()
        while True:
            current = self._request("GET", f"/jobs/{job_id}")
            if current["status"] in ("succeeded", "failed"):
                break
            if time.time() - started > 600:
                raise RuntimeError(f"任务 {job_id} 超时")
            time.sleep(0.5)
        if current["status"] != "succeeded":
            raise RuntimeError(f"任务 {kind} 失败：{current.get('error')}")
        print(f"[job] {kind} 完成 ({job_id})")
        return current["result"]

    def download(self, job_id: str, path: str, filename: str) -> str:
        data = self._request("GET", path, raw=True)
        output = os.path.join(self.out_dir, filename)
        with open(output, "wb") as handle:
            handle.write(data)
        print(f"[export] {output}")
        return output


def iso(moment: dt.datetime) -> str:
    return moment.astimezone(dt.timezone.utc).isoformat().replace("+00:00", "Z")


def build_station() -> dict[str, Any]:
    return {
        "id": "gs-demo-beijing",
        "name": "北京演示站",
        "enabled": True,
        "color": "#ffd54f",
        "latitude_deg": 39.9042,
        "longitude_deg": 116.4074,
        "altitude_km": 0.05,
        "min_elevation_deg": 10,
    }


def main() -> int:
    parser = argparse.ArgumentParser(description="KST Phase 4 API 自动化示例")
    parser.add_argument(
        "--base-url",
        default=os.environ.get("KST_BASE_URL", "http://127.0.0.1:18080"),
        help="后端地址，默认 http://127.0.0.1:18080",
    )
    parser.add_argument("--username", default=os.environ.get("KST_USERNAME", "admin"))
    parser.add_argument("--password", default=os.environ.get("KST_PASSWORD", "admin123"))
    parser.add_argument("--hours", type=float, default=6.0, help="分析时长（小时）")
    parser.add_argument("--out", default="kst_demo_out", help="导出目录")
    parser.add_argument(
        "--keep-scenario",
        action="store_true",
        help="保留示例场景（默认在流程结束后删除）",
    )
    args = parser.parse_args()

    os.makedirs(args.out, exist_ok=True)
    client = KstClient(args.base_url, args.username, args.password)
    client.out_dir = args.out
    client.login()

    start = dt.datetime.now(dt.timezone.utc)
    end = start + dt.timedelta(hours=args.hours)
    start_text, end_text = iso(start), iso(end)

    empty_data = {
        "satellites": [],
        "ground_stations": [build_station()],
        "sensors": [],
        "constellations": [],
    }
    scenario = client.create_scenario("Phase4 API Demo", empty_data)
    scenario_id = scenario["id"]

    generation = client.generate_constellation(
        {
            "name": "Demo-Walker-12/3",
            "constellation_type": "walker_delta",
            "altitude_km": 550.0,
            "inclination_deg": 53.0,
            "total_satellites": 12,
            "planes": 3,
            "phasing": 1,
            "color_by_plane": True,
        }
    )

    data = {
        "satellites": generation["satellites"],
        "ground_stations": [build_station()],
        "sensors": [],
        "constellations": [generation["constellation"]],
    }
    client.update_scenario(scenario_id, "Phase4 API Demo", data)
    satellite_ids = [satellite["id"] for satellite in generation["satellites"]]
    station_id = "gs-demo-beijing"

    propagation = client.run_job(
        scenario_id,
        "propagation",
        {
            "start": start_text,
            "end": end_text,
            "step_seconds": 60,
            "satellite_ids": satellite_ids,
        },
    )
    print(f"[result] 传播轨迹 {len(propagation['satellites'])} 条")

    access = client.run_job(
        scenario_id,
        "access",
        {
            "satellite_id": satellite_ids[0],
            "ground_station_id": station_id,
            "start": start_text,
            "end": end_text,
            "step_seconds": 30,
        },
    )
    print(f"[result] Access 窗口 {access['window_count']} 个")

    analysis = client.run_job(
        scenario_id,
        "constellation_analysis",
        {
            "satellite_ids": satellite_ids,
            "station_ids": [station_id],
            "start": start_text,
            "end": end_text,
            "step_seconds": 60,
            "region": {
                "min_latitude_deg": -60.0,
                "max_latitude_deg": 60.0,
                "min_longitude_deg": -100.0,
                "max_longitude_deg": 140.0,
            },
            "grid_step_deg": 5.0,
            "min_elevation_deg": 10.0,
        },
    )
    print(
        "[result] 覆盖率 {:.1%} / 平均重访 {:.0f}s / 最大间隙 {:.0f}s".format(
            analysis["coverage_ratio"],
            analysis["mean_revisit_seconds"],
            analysis["max_gap_seconds"],
        )
    )

    link_options = {
        "frequency_hz": 2.2e9,
        "tx_power_dbw": 10.0,
        "tx_gain_db": 30.0,
        "rx_gain_db": 35.0,
        "misc_loss_db": 1.0,
        "system_noise_temp_k": 500.0,
        "bandwidth_hz": 1.0e6,
        "required_cn0_dbhz": 60.0,
        "satellite_antenna_pattern": "cosine",
        "satellite_pattern_exponent": 2.0,
        "atmosphere_model": "simple",
        "zenith_attenuation_db": 0.05,
    }
    link = client.run_job(
        scenario_id,
        "link_budget",
        {
            "satellite_id": satellite_ids[0],
            "ground_station_id": station_id,
            "start": start_text,
            "end": end_text,
            "step_seconds": 30,
            "options": link_options,
        },
    )
    print(
        f"[result] 链路最小余量 {link['min_margin_db']:.2f} dB / "
        f"平均大气损耗 {link['mean_atmospheric_loss_db']:.3f} dB"
    )
    link_job_id = client.last_job_id

    radar = client.run_job(
        scenario_id,
        "radar",
        {
            "satellite_id": satellite_ids[0],
            "ground_station_id": station_id,
            "start": start_text,
            "end": end_text,
            "step_seconds": 30,
            "options": {
                "frequency_hz": 3.0e9,
                "tx_power_w": 1000.0,
                "tx_gain_db": 35.0,
                "rx_gain_db": 35.0,
                "system_loss_db": 3.0,
                "noise_figure_db": 3.0,
                "bandwidth_hz": 1.0e6,
                "target_rcs_m2": 1.0,
                "required_snr_db": 13.0,
                "pulses_integrated": 10,
                "min_elevation_deg": 10.0,
            },
        },
    )
    print(f"[result] 雷达最大探测距离 {radar['max_detection_range_km']:.2f} km")
    radar_job_id = client.last_job_id

    interference = client.run_job(
        scenario_id,
        "interference",
        {
            "satellite_id": satellite_ids[0],
            "ground_station_id": station_id,
            "interferer_ids": satellite_ids[1:3],
            "start": start_text,
            "end": end_text,
            "step_seconds": 60,
            "threshold_ci_db": 10.0,
            "options": link_options,
        },
    )
    print(
        f"[result] 同频干扰最小 C/I {interference['min_ci_db']:.2f} dB / "
        f"低门限比例 {interference['degraded_ratio']:.1%}"
    )
    interference_job_id = client.last_job_id

    report = client.run_job(
        scenario_id,
        "report",
        {
            "title": "Phase 4 API Demo 分析报告",
            "start": start_text,
            "end": end_text,
            "step_seconds": 60,
            "satellite_ids": satellite_ids[:6],
            "station_ids": [station_id],
            "region": {
                "min_latitude_deg": -60.0,
                "max_latitude_deg": 60.0,
                "min_longitude_deg": -100.0,
                "max_longitude_deg": 140.0,
            },
            "grid_step_deg": 5.0,
            "min_elevation_deg": 10.0,
            "include_access": True,
            "include_coverage": True,
            "include_link": True,
            "include_radar": True,
        },
    )
    print(f"[result] 报告统计 {json.dumps(report['stats'], ensure_ascii=False)}")
    report_job_id = client.last_job_id

    if report_job_id:
        client.download(
            report_job_id,
            f"/jobs/{report_job_id}/report?format=html",
            "phase4-report.html",
        )
        client.download(
            report_job_id,
            f"/jobs/{report_job_id}/report?format=markdown",
            "phase4-report.md",
        )
    for job_id, filename in [
        (link_job_id, "link-budget.csv"),
        (radar_job_id, "radar.csv"),
        (interference_job_id, "interference.csv"),
    ]:
        if job_id:
            client.download(job_id, f"/jobs/{job_id}/csv", filename)

    print("[done] Phase 4 API 示例流程完成")
    if not args.keep_scenario:
        client.delete_scenario(scenario_id)
    return 0


if __name__ == "__main__":
    try:
        sys.exit(main())
    except Exception as error:  # noqa: BLE001 - 示例脚本直接打印失败原因
        print(f"[error] {error}", file=sys.stderr)
        sys.exit(1)
