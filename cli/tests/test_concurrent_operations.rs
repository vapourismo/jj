// Copyright 2022 The Jujutsu Authors
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
// https://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

use itertools::Itertools as _;

use crate::common::CommandOutput;
use crate::common::TestEnvironment;
use crate::common::TestWorkDir;

#[test]
fn test_concurrent_operation_divergence() {
    let test_env = TestEnvironment::default();
    test_env.run_jj_in(".", ["git", "init", "repo"]).success();
    let work_dir = test_env.work_dir("repo");

    work_dir.run_jj(["describe", "-m", "message 1"]).success();
    work_dir
        .run_jj(["describe", "-m", "message 2", "--at-op", "@-"])
        .success();

    // "--at-op=@" disables op heads merging, and prints head operation ids.
    let output = work_dir.run_jj(["op", "log", "--at-op=@"]);
    insta::assert_snapshot!(output, @r#"
    ------- stderr -------
    Error: The "@" expression resolved to more than one operation
    Hint: Try specifying one of the operations by ID: 0162305507cc, d74dff64472e
    [EOF]
    [exit status: 1]
    "#);

    // "op log --at-op" should work without merging the head operations
    let output = work_dir.run_jj(["op", "log", "--at-op=d74dff64472e"]);
    insta::assert_snapshot!(output, @r"
    @  d74dff64472e test-username@host.example.com 2001-02-03 04:05:09.000 +07:00 - 2001-02-03 04:05:09.000 +07:00
    │  describe commit 230dd059e1b059aefc0da06a2e5a7dbf22362f22
    │  args: jj describe -m 'message 2' --at-op @-
    ○  eac759b9ab75 test-username@host.example.com 2001-02-03 04:05:07.000 +07:00 - 2001-02-03 04:05:07.000 +07:00
    │  add workspace 'default'
    ○  000000000000 root()
    [EOF]
    ");

    // We should be informed about the concurrent modification
    let output = work_dir.run_jj(["log", "-T", "description"]);
    insta::assert_snapshot!(output, @r"
    @  message 1
    │ ○  message 2
    ├─╯
    ◆
    [EOF]
    ------- stderr -------
    Concurrent modification detected, resolving automatically.
    [EOF]
    ");
}

#[test]
fn test_concurrent_operations_auto_rebase() {
    let test_env = TestEnvironment::default();
    test_env.run_jj_in(".", ["git", "init", "repo"]).success();
    let work_dir = test_env.work_dir("repo");

    work_dir.write_file("file", "contents");
    work_dir.run_jj(["describe", "-m", "initial"]).success();
    let output = work_dir.run_jj(["op", "log"]);
    insta::assert_snapshot!(output, @r"
    @  c62ace5c0522 test-username@host.example.com 2001-02-03 04:05:08.000 +07:00 - 2001-02-03 04:05:08.000 +07:00
    │  describe commit 4e8f9d2be039994f589b4e57ac5e9488703e604d
    │  args: jj describe -m initial
    ○  82d32fc68fc3 test-username@host.example.com 2001-02-03 04:05:08.000 +07:00 - 2001-02-03 04:05:08.000 +07:00
    │  snapshot working copy
    │  args: jj describe -m initial
    ○  eac759b9ab75 test-username@host.example.com 2001-02-03 04:05:07.000 +07:00 - 2001-02-03 04:05:07.000 +07:00
    │  add workspace 'default'
    ○  000000000000 root()
    [EOF]
    ");
    let op_id_hex = output.stdout.raw()[3..15].to_string();

    work_dir.run_jj(["describe", "-m", "rewritten"]).success();
    work_dir
        .run_jj(["new", "--at-op", &op_id_hex, "-m", "new child"])
        .success();

    // We should be informed about the concurrent modification
    let output = get_log_output(&work_dir);
    insta::assert_snapshot!(output, @r"
    ○  db141860e12c2d5591c56fde4fc99caf71cec418 new child
    @  07c3641e495cce57ea4ca789123b52f421c57aa2 rewritten
    ◆  0000000000000000000000000000000000000000
    [EOF]
    ------- stderr -------
    Concurrent modification detected, resolving automatically.
    Rebased 1 descendant commits onto commits rewritten by other operation
    [EOF]
    ");
}

#[test]
fn test_concurrent_operations_wc_modified() {
    let test_env = TestEnvironment::default();
    test_env.run_jj_in(".", ["git", "init", "repo"]).success();
    let work_dir = test_env.work_dir("repo");

    work_dir.write_file("file", "contents\n");
    work_dir.run_jj(["describe", "-m", "initial"]).success();
    let output = work_dir.run_jj(["op", "log"]).success();
    let op_id_hex = output.stdout.raw()[3..15].to_string();

    work_dir
        .run_jj(["new", "--at-op", &op_id_hex, "-m", "new child1"])
        .success();
    work_dir
        .run_jj(["new", "--at-op", &op_id_hex, "-m", "new child2"])
        .success();
    work_dir.write_file("file", "modified\n");

    // We should be informed about the concurrent modification
    let output = get_log_output(&work_dir);
    insta::assert_snapshot!(output, @r"
    @  4eadcf3df11f46ef3d825c776496221cc8303053 new child1
    │ ○  68119f1643b7e3c301c5f7c2b6c9bf4ccba87379 new child2
    ├─╯
    ○  2ff7ae858a3a11837fdf9d1a76be295ef53f1bb3 initial
    ◆  0000000000000000000000000000000000000000
    [EOF]
    ------- stderr -------
    Concurrent modification detected, resolving automatically.
    [EOF]
    ");
    let output = work_dir.run_jj(["diff", "--git"]);
    insta::assert_snapshot!(output, @r"
    diff --git a/file b/file
    index 12f00e90b6..2e0996000b 100644
    --- a/file
    +++ b/file
    @@ -1,1 +1,1 @@
    -contents
    +modified
    [EOF]
    ");

    // The working copy should be committed after merging the operations
    let output = work_dir.run_jj(["op", "log", "-Tdescription"]);
    insta::assert_snapshot!(output, @r"
    @  snapshot working copy
    ○    reconcile divergent operations
    ├─╮
    ○ │  new empty commit
    │ ○  new empty commit
    ├─╯
    ○  describe commit 506f4ec3c2c62befa15fabc34ca9d4e6d7bef254
    ○  snapshot working copy
    ○  add workspace 'default'
    ○
    [EOF]
    ");
}

#[test]
fn test_concurrent_snapshot_wc_reloadable() {
    let test_env = TestEnvironment::default();
    test_env.run_jj_in(".", ["git", "init", "repo"]).success();
    let work_dir = test_env.work_dir("repo");
    let op_heads_dir = work_dir
        .root()
        .join(".jj")
        .join("repo")
        .join("op_heads")
        .join("heads");

    work_dir.write_file("base", "");
    work_dir.run_jj(["commit", "-m", "initial"]).success();

    // Create new commit and checkout it.
    work_dir.write_file("child1", "");
    work_dir.run_jj(["commit", "-m", "new child1"]).success();

    let template = r#"id ++ "\n" ++ description ++ "\n" ++ tags"#;
    let output = work_dir.run_jj(["op", "log", "-T", template]);
    insta::assert_snapshot!(output, @r"
    @  ec6bf266624bbaed55833a34ae62fa95c0e9efa651b94eb28846972da645845052dcdc8580332a5628849f23f48b9e99fc728dc3fb13106df8d0666d746f8b85
    │  commit 554d22b2c43c1c47e279430197363e8daabe2fd6
    │  args: jj commit -m 'new child1'
    ○  23858df860b789e8176a73c0eb21804e3f1848f26d68b70d234c004d08980c41499b6669042bca20fbc2543c437222a084c7cd473e91c7a9a095a02bf38544ab
    │  snapshot working copy
    │  args: jj commit -m 'new child1'
    ○  e1db5fa988fc66e5cc0491b00c53fb93e25e730341c850cb42e1e0db0c76d2b4065005787563301b1d292c104f381918897f7deabeb92d2532f42ce75d3fe588
    │  commit de71e09289762a65f80bb1c3dae2a949df6bcde7
    │  args: jj commit -m initial
    ○  7de878155a459b7751097222132c935f9dcbb8f69a72b0f3a9036345a963010a553dc7c92964220128679ead72b087ca3aaf4ab9e20a221d1ffa4f9e92a32193
    │  snapshot working copy
    │  args: jj commit -m initial
    ○  eac759b9ab75793fd3da96e60939fb48f2cd2b2a9c1f13ffe723cf620f3005b8d3e7e923634a07ea39513e4f2f360c87b9ad5d331cf90d7a844864b83b72eba1
    │  add workspace 'default'
    ○  00000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000

    [EOF]
    ");
    let op_log_lines = output.stdout.raw().lines().collect_vec();
    let current_op_id = op_log_lines[0].split_once("  ").unwrap().1;
    let previous_op_id = op_log_lines[6].split_once("  ").unwrap().1;

    // Another process started from the "initial" operation, but snapshots after
    // the "child1" checkout has been completed.
    std::fs::rename(
        op_heads_dir.join(current_op_id),
        op_heads_dir.join(previous_op_id),
    )
    .unwrap();
    work_dir.write_file("child2", "");
    let output = work_dir.run_jj(["describe", "-m", "new child2"]);
    insta::assert_snapshot!(output, @r"
    ------- stderr -------
    Working copy  (@) now at: kkmpptxz 1795621b new child2
    Parent commit (@-)      : rlvkpnrz 86f54245 new child1
    [EOF]
    ");

    // Since the repo can be reloaded before snapshotting, "child2" should be
    // a child of "child1", not of "initial".
    let template = r#"commit_id ++ " " ++ description"#;
    let output = work_dir.run_jj(["log", "-T", template, "-s"]);
    insta::assert_snapshot!(output, @r"
    @  1795621b54f4ebb435978b65d66bc0f90d8f20b6 new child2
    │  A child2
    ○  86f54245e13f850f8275b5541e56da996b6a47b7 new child1
    │  A child1
    ○  84f07f6bca2ffeddac84a8b09f60c6b81112375c initial
    │  A base
    ◆  0000000000000000000000000000000000000000
    [EOF]
    ");
}

#[must_use]
fn get_log_output(work_dir: &TestWorkDir) -> CommandOutput {
    let template = r#"commit_id ++ " " ++ description"#;
    work_dir.run_jj(["log", "-T", template])
}
