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

//! The names a soundbank lists must be the files the compiler writes.
//!
//! These change the process working directory; run with `--test-threads=1`.

mod common;

use am::assets::SoundbankList;
use am::compiler::{get_conversion_entries, output_path_for};
use common::soundbank_helpers::*;

#[tokio::test]
async fn test_every_listed_name_is_the_file_the_compiler_writes() {
    let fx = fixture("sb_compiler");
    create_ok(
        &fx,
        &[
            "--sound", "boom,step", "--collection", "crowd", "--event", "hit", "--switch",
            "surface", "--switch-container", "footsteps", "--effect", "echo", "--attenuator",
            "room", "--rtpc", "wind",
        ],
    )
    .await;
    let sources = fx.sources_dir();
    let build = fx.project_root().join("build");
    for list in SoundbankList::ALL {
        let dir = list.asset_type().directory_name();
        let entry = get_conversion_entries()
            .into_iter()
            .find(|e| e.subdir == dir)
            .unwrap_or_else(|| panic!("no conversion entry for {dir}"));
        for listed in entries(&fx, "bank", list.field()) {
            let source = list.source_relative(&listed).unwrap();
            let compiled = output_path_for(
                &sources.join(dir).join(&source),
                &sources,
                &build,
                &entry.output_extension,
            );
            assert_eq!(
                compiled,
                build.join(dir).join(&listed),
                "{} entry '{listed}' is not what `am build` writes",
                list.field()
            );
        }
    }
}

#[test]
fn test_effects_compile_as_amfx() {
    let entry = get_conversion_entries()
        .into_iter()
        .find(|e| e.subdir == "effects")
        .unwrap();
    assert_eq!(entry.output_extension, ".amfx");
    assert_eq!(SoundbankList::Effects.extension(), ".amfx");
}
