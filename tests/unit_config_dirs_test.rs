// Copyright (c) 2026-present Sparky Studios. All rights reserved.
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

//! Where bus and engine config files are looked up: the sources dir first,
//! the project root as a legacy fallback, each file once.

mod common;

use am::assets::bus::{bus_file_path, scan_bus_files};
use am::assets::engine_config::{engine_config_path, scan_engine_configs};
use am::common::utils::{config_write_dir, project_config_dirs};
use common::fixtures::AssetTestFixture;
use serde_json::json;

fn master_only() -> serde_json::Value {
    json!([{ "id": 1, "name": "master", "gain": 1.0, "fader": "Linear" }])
}

#[test]
fn test_dirs_are_sources_then_root() {
    let fx = AssetTestFixture::new("dirs").unwrap();
    assert_eq!(
        project_config_dirs(fx.project_root()),
        vec![fx.sources_dir(), fx.project_root().to_path_buf()]
    );
    assert_eq!(config_write_dir(fx.project_root()), fx.sources_dir());
}

#[test]
fn test_empty_sources_dir_means_the_root_only() {
    let fx = AssetTestFixture::new("dirs_root").unwrap();
    let amproject = json!({
        "name": "dirs_root", "default_configuration": "pc.config.amconfig",
        "sources_dir": "", "data_dir": "data", "build_dir": "build", "version": 1
    });
    std::fs::write(
        fx.project_root().join(".amproject"),
        serde_json::to_string(&amproject).unwrap(),
    )
    .unwrap();
    assert_eq!(
        project_config_dirs(fx.project_root()),
        vec![fx.project_root().to_path_buf()]
    );
    assert_eq!(config_write_dir(fx.project_root()), fx.project_root());
}

#[test]
fn test_new_files_are_written_in_the_sources_dir() {
    let fx = AssetTestFixture::new("write").unwrap();
    assert_eq!(
        bus_file_path(fx.project_root(), "x"),
        fx.sources_dir().join("x.buses.json")
    );
    assert_eq!(
        engine_config_path(fx.project_root(), "x"),
        fx.sources_dir().join("x.config.json")
    );
}

#[test]
fn test_root_bus_files_are_read_as_a_fallback() {
    let fx = AssetTestFixture::new("legacy").unwrap();
    fx.write_root_buses_file("legacy.buses.json", master_only())
        .unwrap();
    let names: Vec<String> = scan_bus_files(fx.project_root())
        .unwrap()
        .files
        .into_iter()
        .map(|f| f.name)
        .collect();
    assert_eq!(names, vec!["legacy".to_string(), "main".to_string()]);
}

#[test]
fn test_a_file_present_in_both_places_is_read_once_from_sources() {
    let fx = AssetTestFixture::new("dup").unwrap();
    fx.write_root_buses_file("main.buses.json", master_only())
        .unwrap();
    let scan = scan_bus_files(fx.project_root()).unwrap();
    assert_eq!(scan.files.len(), 1);
    assert_eq!(scan.files[0].path, fx.sources_dir().join("main.buses.json"));
}

#[test]
fn test_root_engine_configs_are_read_and_deduplicated() {
    let fx = AssetTestFixture::new("cfg").unwrap();
    let body = r#"{"buses_file": "main.buses.ambus"}"#;
    std::fs::write(fx.project_root().join("legacy.config.json"), body).unwrap();
    std::fs::write(fx.project_root().join("pc.config.json"), body).unwrap();
    std::fs::write(fx.sources_dir().join("pc.config.json"), body).unwrap();
    let scan = scan_engine_configs(fx.project_root()).unwrap();
    let names: Vec<&str> = scan.files.iter().map(|f| f.name.as_str()).collect();
    assert_eq!(names, vec!["legacy", "pc"]);
    let pc = scan.files.iter().find(|f| f.name == "pc").unwrap();
    assert_eq!(pc.path, fx.sources_dir().join("pc.config.json"));
}
