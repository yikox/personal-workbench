use super::*;

fn temp_root() -> tempfile::TempDir {
    tempfile::tempdir().expect("tempdir")
}

#[test]
fn project_entries_create_only_inside_real_unmanaged_project_directories() {
    let td = temp_root();
    let root = td.path().join("w");
    init_workspace(&root).unwrap();
    let project = create_project(&root, "Project", "").unwrap();
    let nested = root.join(&project.path).join("src");
    fs::create_dir(&nested).unwrap();

    create_project_entry(&root, &project.id, &project.path, "notes.md", false).unwrap();
    create_project_entry(&root, &project.id, "项目/Project/src", "module", true).unwrap();
    fs::create_dir(nested.join("关联文档")).unwrap();
    create_project_entry(
        &root,
        &project.id,
        "项目/Project/src/关联文档",
        "ordinary.txt",
        false,
    )
    .unwrap();
    assert!(root.join(&project.path).join("notes.md").is_file());
    assert!(nested.join("module").is_dir());
    assert!(nested.join("关联文档/ordinary.txt").is_file());

    fs::write(root.join(&project.path).join("occupied"), "keep").unwrap();
    assert!(create_project_entry(&root, &project.id, &project.path, "occupied", false).is_err());
    assert_eq!(
        fs::read_to_string(root.join(&project.path).join("occupied")).unwrap(),
        "keep"
    );

    let outside = td.path().join("outside");
    fs::create_dir(&outside).unwrap();
    for invalid_parent in ["../outside", outside.to_str().unwrap(), "文档"] {
        assert!(create_project_entry(&root, &project.id, invalid_parent, "bad", false).is_err());
    }
    assert!(create_project_entry(&root, &project.id, &project.path, MARKER, false).is_err());
    assert!(create_project_entry(&root, &project.id, &project.path, ".git", true).is_err());
    assert!(create_project_entry(
        &root,
        &project.id,
        &format!("{}/关联文档", project.path),
        "bad.md",
        false,
    )
    .is_err());

    #[cfg(unix)]
    {
        let link = root.join(&project.path).join("linked");
        std::os::unix::fs::symlink(&nested, &link).unwrap();
        assert!(create_project_entry(
            &root,
            &project.id,
            &format!("{}/linked", project.path),
            "bad",
            false,
        )
        .is_err());
        let git_dir = nested.join(".git");
        fs::create_dir(&git_dir).unwrap();
        assert!(
            create_project_entry(&root, &project.id, "项目/Project/src/.git", "bad", false,)
                .is_err()
        );
        assert!(fs::read_dir(nested)
            .unwrap()
            .all(|entry| entry.unwrap().file_name() != "bad"));
    }
}

#[test]
fn markdown_discovery_skips_hidden_generated_and_symlink_paths() {
    let td = temp_root();
    let root = td.path().join("w");
    init_workspace(&root).unwrap();
    let project = create_project(&root, "Docs", "").unwrap();
    let dir = root.join(&project.path);
    fs::create_dir_all(dir.join("notes/nested")).unwrap();
    fs::create_dir_all(dir.join(".git")).unwrap();
    fs::create_dir_all(dir.join("node_modules/pkg")).unwrap();
    fs::write(dir.join("notes/nested/guide.MD"), "guide").unwrap();
    fs::write(dir.join(".hidden.md"), "hidden").unwrap();
    fs::write(dir.join(".git/secret.md"), "git").unwrap();
    fs::write(dir.join("node_modules/pkg/readme.md"), "dep").unwrap();
    fs::write(dir.join("ordinary.txt"), "text").unwrap();
    #[cfg(unix)]
    {
        let outside = td.path().join("outside.md");
        fs::write(&outside, "outside").unwrap();
        std::os::unix::fs::symlink(&outside, dir.join("linked.md")).unwrap();
    }
    let found = discover_markdown(&root, &project.id).unwrap();
    assert_eq!(
        found
            .iter()
            .map(|doc| doc.path.as_str())
            .collect::<Vec<_>>(),
        ["notes/nested/guide.MD"]
    );
}

#[test]
fn markdown_discovery_includes_only_registered_repository_links() {
    let td = temp_root();
    let root = td.path().join("w");
    init_workspace(&root).unwrap();
    let project = create_project(&root, "Docs", "").unwrap();
    let repo = create_resource(&root, ResourceKind::Repository, "Repo").unwrap();
    fs::write(root.join(&repo.path).join("ai-workbench-plan.md"), "plan").unwrap();
    fs::create_dir_all(root.join(&repo.path).join(".git")).unwrap();
    fs::write(root.join(&repo.path).join(".git/config.md"), "git").unwrap();
    let unregistered = create_resource(&root, ResourceKind::Repository, "Other").unwrap();
    fs::write(root.join(&unregistered.path).join("private.md"), "private").unwrap();
    link_resource(
        &root,
        &project.id,
        &project.revision,
        ResourceKind::Repository,
        &repo.path,
        None,
    )
    .unwrap();
    let found = discover_markdown(&root, &project.id).unwrap();
    assert_eq!(
        found
            .iter()
            .map(|doc| doc.path.as_str())
            .collect::<Vec<_>>(),
        ["关联仓库/Repo/ai-workbench-plan.md"]
    );
}

#[test]
fn document_original_path_link_is_readable_and_unlink_keeps_managed_document() {
    let td = temp_root();
    let root = td.path().join("w");
    init_workspace(&root).unwrap();
    let mut project = create_project(&root, "Docs", "").unwrap();
    let dir = root.join(&project.path);
    fs::create_dir_all(dir.join("notes/nested")).unwrap();
    let source = dir.join("notes/nested/guide.md");
    fs::write(&source, "managed content").unwrap();
    let resource = create_resource(&root, ResourceKind::Document, "guide.md").unwrap();
    fs::write(root.join(&resource.path), "managed content").unwrap();
    project = link_resource(
        &root,
        &project.id,
        &project.revision,
        ResourceKind::Document,
        &resource.path,
        None,
    )
    .unwrap();
    fs::remove_file(&source).unwrap();
    #[cfg(unix)]
    std::os::unix::fs::symlink(root.join(&resource.path), &source).unwrap();
    project = set_document_source_path(&root, &project.id, &resource.path, "notes/nested/guide.md")
        .unwrap();
    snapshot(&root).unwrap();
    assert_eq!(
        fs::read_to_string(
            resolve_workspace_path(&root, &format!("{}/notes/nested/guide.md", project.path))
                .unwrap()
        )
        .unwrap(),
        "managed content"
    );
    let renamed =
        rename_resource(&root, ResourceKind::Document, &resource.path, "renamed.md").unwrap();
    project = snapshot(&root).unwrap().projects.remove(0);
    assert_eq!(fs::read_link(&source).unwrap(), root.join(&renamed.path));
    assert_eq!(
        fs::read_to_string(
            resolve_workspace_path(&root, &format!("{}/notes/nested/guide.md", project.path))
                .unwrap()
        )
        .unwrap(),
        "managed content"
    );
    let remaining = unlink_resource(
        &root,
        &project.id,
        &project.revision,
        ResourceKind::Document,
        &renamed.path,
    )
    .unwrap();
    assert!(fs::symlink_metadata(source).is_err());
    assert_eq!(
        fs::read_to_string(root.join(&renamed.path)).unwrap(),
        "managed content"
    );
    assert!(remaining.resources.is_empty());
}

#[test]
fn staged_resources_require_valid_location_type_and_exclusive_destination() {
    let td = temp_root();
    let root = td.path().join("w");
    init_workspace(&root).unwrap();
    let staged = |root: &Path| root.join(format!(".personal-workbench-stage-{}", Uuid::new_v4()));

    let repository_stage = staged(&root);
    fs::create_dir(&repository_stage).unwrap();
    fs::write(repository_stage.join("keep"), "repo").unwrap();
    let repository =
        install_staged_resource(&root, ResourceKind::Repository, "repo", &repository_stage)
            .unwrap();
    assert_eq!(repository.path, "仓库/repo");
    assert_eq!(
        fs::read_to_string(root.join(&repository.path).join("keep")).unwrap(),
        "repo"
    );
    assert!(!repository_stage.exists());

    let attachment_stage = staged(&root);
    fs::write(&attachment_stage, "attachment").unwrap();
    let attachment = install_staged_resource(
        &root,
        ResourceKind::Attachment,
        "file.bin",
        &attachment_stage,
    )
    .unwrap();
    assert_eq!(attachment.path, "附件/file.bin");
    assert_eq!(
        fs::read_to_string(root.join(&attachment.path)).unwrap(),
        "attachment"
    );

    let mismatch = staged(&root);
    fs::create_dir(&mismatch).unwrap();
    assert!(install_staged_resource(&root, ResourceKind::Attachment, "wrong", &mismatch).is_err());
    assert!(mismatch.is_dir());
    assert!(install_staged_resource(&root, ResourceKind::Document, "doc", &mismatch).is_err());

    let collision = staged(&root);
    fs::write(&collision, "source remains").unwrap();
    fs::write(root.join("附件/taken"), "destination remains").unwrap();
    assert!(install_staged_resource(&root, ResourceKind::Attachment, "taken", &collision).is_err());
    assert_eq!(fs::read_to_string(&collision).unwrap(), "source remains");
    assert_eq!(
        fs::read_to_string(root.join("附件/taken")).unwrap(),
        "destination remains"
    );

    let misplaced = td
        .path()
        .join(format!(".personal-workbench-stage-{}", Uuid::new_v4()));
    fs::write(&misplaced, "stay").unwrap();
    assert!(
        install_staged_resource(&root, ResourceKind::Attachment, "misplaced", &misplaced).is_err()
    );
    assert!(misplaced.exists());
}

#[cfg(target_os = "macos")]
#[test]
fn fixed_directory_fds_resist_parent_replacement_with_external_symlinks() {
    let td = temp_root();
    let root = td.path().join("w");
    init_workspace(&root).unwrap();
    let project = create_project(&root, "Project", "").unwrap();
    let parent_path = root.join(&project.path).join("src");
    fs::create_dir(&parent_path).unwrap();
    let outside = td.path().join("outside");
    fs::create_dir(&outside).unwrap();

    let root_fd = open_directory(&root).unwrap();
    let project_fd = open_directory_chain(&root_fd, Path::new(&project.path)).unwrap();
    let parent_fd = open_directory_at(&project_fd, std::ffi::OsStr::new("src")).unwrap();
    let detached_parent = root.join(&project.path).join("src-detached");
    fs::rename(&parent_path, &detached_parent).unwrap();
    std::os::unix::fs::symlink(&outside, &parent_path).unwrap();
    create_entry_at(&parent_fd, "created.txt", false).unwrap();
    assert!(detached_parent.join("created.txt").is_file());
    assert!(!outside.join("created.txt").exists());

    let stage_name = format!(".personal-workbench-stage-{}", Uuid::new_v4());
    let stage = root.join(&stage_name);
    fs::write(&stage, "staged").unwrap();
    let attachment_dir = root.join("附件");
    let attachment_fd = open_directory(&attachment_dir).unwrap();
    let detached_attachment = root.join("附件-detached");
    fs::rename(&attachment_dir, &detached_attachment).unwrap();
    let outside_attachment = outside.join("附件");
    fs::create_dir(&outside_attachment).unwrap();
    fs::write(outside_attachment.join("target"), "preserve").unwrap();
    std::os::unix::fs::symlink(&outside_attachment, &attachment_dir).unwrap();

    rename_at_exclusive(&root_fd, &stage_name, &attachment_fd, "target").unwrap();
    assert_eq!(
        fs::read_to_string(detached_attachment.join("target")).unwrap(),
        "staged"
    );
    assert_eq!(
        fs::read_to_string(outside_attachment.join("target")).unwrap(),
        "preserve"
    );
    assert!(!stage.exists());
}

#[test]
fn initializes_new_root_and_refuses_unmarked_nonempty_directory() {
    let td = temp_root();
    let root = td.path().join("workspace");
    let snap = init_workspace(&root).unwrap();
    assert!(snap.projects.is_empty() && snap.resources.is_empty());
    for d in CATEGORIES {
        assert!(root.join(d).is_dir());
    }
    assert!(root.join(TEMP_CATEGORY).is_dir());

    let other = td.path().join("existing");
    fs::create_dir(&other).unwrap();
    fs::write(other.join("keep.txt"), "keep").unwrap();
    assert!(init_workspace(&other).is_err());
    assert_eq!(fs::read_to_string(other.join("keep.txt")).unwrap(), "keep");
}

#[test]
fn temporary_directories_create_rename_conflict_and_trash_without_touching_project_resources() {
    let td = temp_root();
    let root = td.path().join("w");
    init_workspace(&root).unwrap();
    let project = create_project(&root, "Project", "").unwrap();
    let resource = create_resource(&root, ResourceKind::Document, "keep.md").unwrap();
    let project = link_resource(
        &root,
        &project.id,
        &project.revision,
        resource.kind,
        &resource.path,
        None,
    )
    .unwrap();
    let temporary = create_temporary(&root, "Scratch").unwrap();
    let marker_before = fs::read(root.join(&temporary.path).join(TEMP_MARKER)).unwrap();
    fs::write(
        root.join(&temporary.path).join("AGENTS.md"),
        "local instructions",
    )
    .unwrap();
    fs::create_dir(root.join(&temporary.path).join("session-history")).unwrap();

    assert!(create_temporary(&root, "Scratch").is_err());
    let occupied = root.join(TEMP_CATEGORY).join("Occupied");
    fs::create_dir(&occupied).unwrap();
    fs::write(occupied.join("keep.txt"), "keep").unwrap();
    assert!(rename_temporary(&root, &temporary.id, &temporary.revision, "Occupied").is_err());
    assert_eq!(
        fs::read_to_string(occupied.join("keep.txt")).unwrap(),
        "keep"
    );

    let renamed = rename_temporary(&root, &temporary.id, &temporary.revision, "Renamed").unwrap();
    assert_eq!(renamed.path, "临时/Renamed");
    assert!(root.join(&renamed.path).join("AGENTS.md").is_file());
    assert_ne!(
        fs::read(root.join(&renamed.path).join(TEMP_MARKER)).unwrap(),
        marker_before
    );
    assert!(write_text(
        &root,
        &format!("{}/{}", renamed.path, TEMP_MARKER),
        "bad",
        Some("x")
    )
    .is_err());

    let mut called = false;
    delete_temporary(&root, &renamed.id, &renamed.revision, |path| {
        called = true;
        assert_eq!(path, root.join(&renamed.path));
        assert!(path.join("session-history").exists());
        assert!(path.join("AGENTS.md").exists());
        Ok(())
    })
    .unwrap();
    assert!(called);
    assert!(root.join(&renamed.path).exists()); // fake trash does not remove it
    let after = snapshot(&root).unwrap();
    assert_eq!(after.temporary_directories, vec![renamed]);
    assert_eq!(after.resources, vec![resource]);
    assert_eq!(after.projects[0].resources, project.resources);
}

#[test]
fn old_workspace_gets_only_an_empty_temporary_category_on_demand() {
    let td = temp_root();
    let root = td.path().join("old");
    init_workspace(&root).unwrap();
    let p = create_project(&root, "P", "existing").unwrap();
    let r = create_resource(&root, ResourceKind::Script, "run.sh").unwrap();
    let p = link_resource(&root, &p.id, &p.revision, r.kind, &r.path, None).unwrap();
    fs::remove_dir(root.join(TEMP_CATEGORY)).unwrap();
    let snap = snapshot(&root).unwrap();
    assert!(root.join(TEMP_CATEGORY).is_dir());
    assert!(snap.temporary_directories.is_empty());
    assert_eq!(snap.projects[0].resources, p.resources);
    assert_eq!(snap.resources, vec![r]);

    fs::remove_dir(root.join(TEMP_CATEGORY)).unwrap();
    fs::write(root.join(TEMP_CATEGORY), "preserve").unwrap();
    assert!(snapshot(&root).is_err());
    assert_eq!(
        fs::read_to_string(root.join(TEMP_CATEGORY)).unwrap(),
        "preserve"
    );
}

#[test]
fn temporary_text_access_is_utf8_revision_checked_and_rejects_external_symlinks() {
    let td = temp_root();
    let root = td.path().join("w");
    init_workspace(&root).unwrap();
    let temp = create_temporary(&root, "Scratch").unwrap();
    let file = root.join(&temp.path).join("notes.md");
    fs::write(&file, "第一版").unwrap();
    let relative = format!("{}/notes.md", temp.path);
    let doc = read_text(&root, &relative).unwrap();
    write_text(&root, &relative, "更新", Some(&doc.revision)).unwrap();
    assert_eq!(read_text(&root, &relative).unwrap().content, "更新");
    assert!(write_text(&root, &relative, "bad", Some(&doc.revision)).is_err());
    assert!(write_text(
        &root,
        &format!("{}/.git/config", temp.path),
        "bad",
        Some("x")
    )
    .is_err());

    let outside = td.path().join("outside.txt");
    fs::write(&outside, "outside").unwrap();
    #[cfg(unix)]
    {
        std::os::unix::fs::symlink(&outside, root.join(&temp.path).join("escape.txt")).unwrap();
        assert!(read_text(&root, &format!("{}/escape.txt", temp.path)).is_err());
        assert!(resolve_workspace_path(&root, &format!("{}/escape.txt", temp.path)).is_err());
    }
    assert_eq!(fs::read_to_string(outside).unwrap(), "outside");
}

#[test]
fn temporary_revision_and_symlink_name_conflicts_are_preserved() {
    let td = temp_root();
    let root = td.path().join("w");
    init_workspace(&root).unwrap();
    let temp = create_temporary(&root, "Scratch").unwrap();
    fs::write(root.join(&temp.path).join(TEMP_MARKER), "external edit").unwrap();
    assert!(rename_temporary(&root, &temp.id, &temp.revision, "Renamed").is_err());
    assert!(delete_temporary(&root, &temp.id, &temp.revision, |_| {
        panic!("stale temporary revision must not call trash")
    })
    .is_err());
    assert_eq!(
        fs::read_to_string(root.join(&temp.path).join(TEMP_MARKER)).unwrap(),
        "external edit"
    );
    fs::remove_dir_all(root.join(&temp.path)).unwrap();

    let other = td.path().join("external");
    fs::create_dir(&other).unwrap();
    #[cfg(unix)]
    {
        let clean = create_temporary(&root, "Clean").unwrap();
        std::os::unix::fs::symlink(&other, root.join(TEMP_CATEGORY).join("Taken")).unwrap();
        assert!(rename_temporary(&root, &clean.id, &clean.revision, "Taken").is_err());
        assert!(fs::symlink_metadata(root.join(TEMP_CATEGORY).join("Taken"))
            .unwrap()
            .file_type()
            .is_symlink());
        assert!(other.is_dir());
    }
}

#[test]
fn shared_resource_links_and_unlink_only_removes_the_selected_link() {
    let td = temp_root();
    let root = td.path().join("w");
    init_workspace(&root).unwrap();
    let a = create_project(&root, "A", "").unwrap();
    let b = create_project(&root, "B", "").unwrap();
    let r = create_resource(&root, ResourceKind::Attachment, "asset").unwrap();
    let a = link_resource(&root, &a.id, &a.revision, r.kind, &r.path, None).unwrap();
    let b = link_resource(&root, &b.id, &b.revision, r.kind, &r.path, None).unwrap();
    unlink_resource(&root, &a.id, &a.revision, r.kind, &r.path).unwrap();
    assert!(!root.join(&a.path).join("关联附件/asset").exists());
    assert!(root.join(&b.path).join("关联附件/asset").exists());
    assert!(root.join(&r.path).exists());
}

#[test]
fn resource_rename_updates_every_project_and_link() {
    let td = temp_root();
    let root = td.path().join("w");
    init_workspace(&root).unwrap();
    let p1 = create_project(&root, "P1", "").unwrap();
    let p2 = create_project(&root, "P2", "").unwrap();
    let r = create_resource(&root, ResourceKind::Document, "old.md").unwrap();
    link_resource(&root, &p1.id, &p1.revision, r.kind, &r.path, None).unwrap();
    link_resource(&root, &p2.id, &p2.revision, r.kind, &r.path, None).unwrap();
    let renamed = rename_resource(&root, r.kind, &r.path, "new.md").unwrap();
    assert_eq!(renamed.path, "文档/new.md");
    let snap = snapshot(&root).unwrap();
    assert!(snap
        .projects
        .iter()
        .all(|p| p.resources[0].path == renamed.path));
    assert!(root.join("项目/P1/关联文档/new.md").exists());
    assert!(root.join("项目/P2/关联文档/new.md").exists());
}

#[test]
fn text_revision_rejects_external_edits_and_preserves_new_content() {
    let td = temp_root();
    let root = td.path().join("w");
    init_workspace(&root).unwrap();
    let r = create_resource(&root, ResourceKind::Document, "note.md").unwrap();
    let first = read_text(&root, &r.path).unwrap();
    write_text(&root, &r.path, "ours", Some(&first.revision)).unwrap();
    fs::write(root.join(&r.path), "external").unwrap();
    assert!(write_text(&root, &r.path, "overwrite", Some(&first.revision)).is_err());
    assert_eq!(fs::read_to_string(root.join(&r.path)).unwrap(), "external");
}

#[test]
fn rejects_duplicate_targets_and_paths_outside_workspace_without_damage() {
    let td = temp_root();
    let root = td.path().join("w");
    init_workspace(&root).unwrap();
    let p = create_project(&root, "P", "").unwrap();
    let r = create_resource(&root, ResourceKind::Attachment, "a").unwrap();
    let p = link_resource(&root, &p.id, &p.revision, r.kind, &r.path, None).unwrap();
    assert!(link_resource(&root, &p.id, &p.revision, r.kind, &r.path, None).is_err());
    assert!(link_resource(&root, &p.id, &p.revision, r.kind, "../outside", None).is_err());
    assert!(rename_resource(&root, r.kind, &r.path, "../outside").is_err());
    assert!(root.join(&r.path).exists());
    assert_eq!(
        fs::read_link(root.join("项目/P/关联附件/a")).unwrap(),
        root.join(&r.path)
    );
}

#[test]
fn project_writes_require_current_revision_and_delete_uses_trash_callback() {
    let td = temp_root();
    let root = td.path().join("w");
    init_workspace(&root).unwrap();
    let p = create_project(&root, "P", "before").unwrap();
    let newer = update_project(&root, &p.id, &p.revision, "P", "after").unwrap();
    assert!(update_project(&root, &p.id, &p.revision, "P", "stale").is_err());
    let path = root.join(&newer.path);
    assert!(delete_project(&root, &newer.id, &newer.revision, |_| Err(
        "test failure".into()
    ))
    .is_err());
    assert!(path.exists());
    delete_project(&root, &newer.id, &newer.revision, |p| {
        fs::remove_dir_all(p).map_err(|e| e.to_string())
    })
    .unwrap();
    assert!(!path.exists());
}

#[test]
fn exclusive_rename_never_overwrites_an_existing_target() {
    let td = temp_root();
    let source = td.path().join("source.txt");
    let target = td.path().join("target.txt");
    fs::write(&source, "source data").unwrap();
    fs::write(&target, "target data").unwrap();
    assert!(rename_exclusive(&source, &target).is_err());
    assert_eq!(fs::read_to_string(&source).unwrap(), "source data");
    assert_eq!(fs::read_to_string(&target).unwrap(), "target data");
}

#[test]
fn case_only_project_resource_and_alias_renames_work_when_filesystem_supports_them() {
    let td = temp_root();
    let probe_upper = td.path().join("CaseProbe");
    let probe_lower = td.path().join("caseprobe");
    fs::write(&probe_upper, "probe").unwrap();
    let case_insensitive = fs::symlink_metadata(&probe_lower).is_ok();
    fs::remove_file(&probe_upper).unwrap();
    if !case_insensitive {
        eprintln!("skipping case-only rename test: filesystem is case-sensitive");
        return;
    }

    let root = td.path().join("w");
    init_workspace(&root).unwrap();
    let project = create_project(&root, "ProjectCase", "").unwrap();
    let document = create_resource(&root, ResourceKind::Document, "note.md").unwrap();
    let project = link_resource(
        &root,
        &project.id,
        &project.revision,
        document.kind,
        &document.path,
        None,
    )
    .unwrap();
    let project = rename_project(&root, &project.id, &project.revision, "projectcase").unwrap();
    assert_eq!(project.name, "projectcase");
    assert_eq!(project.path, "项目/projectcase");

    let renamed = rename_resource(&root, document.kind, &document.path, "NOTE.md").unwrap();
    assert_eq!(renamed.name, "NOTE.md");
    assert_eq!(renamed.path, "文档/NOTE.md");
    let updated = snapshot(&root).unwrap().projects.remove(0);
    assert_eq!(updated.resources[0].path, renamed.path);
    assert_eq!(updated.resources[0].alias, "NOTE.md");
    assert!(
        fs::symlink_metadata(root.join("项目/projectcase/关联文档/NOTE.md"))
            .unwrap()
            .file_type()
            .is_symlink()
    );
}

#[test]
fn trash_callback_failure_does_not_change_resource_or_associations() {
    let td = temp_root();
    let root = td.path().join("w");
    init_workspace(&root).unwrap();
    let p = create_project(&root, "P", "").unwrap();
    let r = create_resource(&root, ResourceKind::Attachment, "a").unwrap();
    let p = link_resource(&root, &p.id, &p.revision, r.kind, &r.path, None).unwrap();
    let trash_called = std::cell::Cell::new(false);
    assert!(delete_resource(&root, r.kind, &r.path, |_| {
        trash_called.set(true);
        Ok(())
    })
    .unwrap_err()
    .contains("请先解除"));
    assert!(!trash_called.get());
    assert!(root.join(&r.path).exists());
    assert_eq!(snapshot(&root).unwrap().projects[0].resources.len(), 1);
    unlink_resource(&root, &p.id, &p.revision, r.kind, &r.path).unwrap();
    delete_resource(&root, r.kind, &r.path, |_| Err("denied".into())).unwrap_err();
    assert!(root.join(&r.path).exists());
    assert!(snapshot(&root).unwrap().projects[0].resources.is_empty());
}

#[test]
fn attachment_is_a_file_that_can_be_listed_linked_and_read() {
    let td = temp_root();
    let root = td.path().join("w");
    init_workspace(&root).unwrap();
    let project = create_project(&root, "P", "").unwrap();
    let attachment = create_resource(&root, ResourceKind::Attachment, "receipt.txt").unwrap();
    fs::write(root.join(&attachment.path), "sample attachment").unwrap();
    assert!(root.join(&attachment.path).is_file());
    assert!(snapshot(&root)
        .unwrap()
        .resources
        .iter()
        .any(|r| r.path == attachment.path));
    let project = link_resource(
        &root,
        &project.id,
        &project.revision,
        ResourceKind::Attachment,
        &attachment.path,
        None,
    )
    .unwrap();
    assert_eq!(project.resources.len(), 1);
    assert_eq!(
        read_text(&root, &attachment.path).unwrap().content,
        "sample attachment"
    );
}

#[test]
fn managed_project_symlink_can_be_read_and_saved_but_escape_symlinks_are_rejected() {
    let td = temp_root();
    let root = td.path().join("w");
    init_workspace(&root).unwrap();
    let project = create_project(&root, "P", "").unwrap();
    let document = create_resource(&root, ResourceKind::Document, "note.md").unwrap();
    let project = link_resource(
        &root,
        &project.id,
        &project.revision,
        ResourceKind::Document,
        &document.path,
        None,
    )
    .unwrap();
    let alias_path = format!("{}/关联文档/note.md", project.path);
    let read = read_text(&root, &alias_path).unwrap();
    let saved = write_text(&root, &alias_path, "edited via link", Some(&read.revision)).unwrap();
    assert_eq!(saved.content, "edited via link");
    assert!(fs::symlink_metadata(root.join(&alias_path))
        .unwrap()
        .file_type()
        .is_symlink());
    assert_eq!(
        fs::read_to_string(root.join(&document.path)).unwrap(),
        "edited via link"
    );

    let outside = td.path().join("outside.md");
    fs::write(&outside, "outside data").unwrap();
    #[cfg(unix)]
    std::os::unix::fs::symlink(&outside, root.join("文档/escape.md")).unwrap();
    #[cfg(unix)]
    assert!(read_text(&root, "文档/escape.md").is_err());
    #[cfg(unix)]
    assert!(write_text(&root, "文档/escape.md", "damage", Some("anything")).is_err());
    assert_eq!(fs::read_to_string(outside).unwrap(), "outside data");
}

#[test]
fn snapshot_reconciles_links_to_match_project_config() {
    let td = temp_root();
    let root = td.path().join("w");
    init_workspace(&root).unwrap();
    let project = create_project(&root, "P", "").unwrap();
    let resource = create_resource(&root, ResourceKind::Attachment, "a.txt").unwrap();
    link_resource(
        &root,
        &project.id,
        &project.revision,
        resource.kind,
        &resource.path,
        None,
    )
    .unwrap();
    let config_path = root.join(&project.path).join(MARKER);
    let mut config: ProjectConfig =
        serde_json::from_slice(&fs::read(&config_path).unwrap()).unwrap();
    config.resources.clear();
    fs::write(config_path, serde_json::to_vec_pretty(&config).unwrap()).unwrap();
    snapshot(&root).unwrap();
    assert!(fs::symlink_metadata(root.join(&project.path).join("关联附件/a.txt")).is_err());
}

#[test]
fn snapshot_creates_and_updates_links_after_external_config_edits() {
    let td = temp_root();
    let root = td.path().join("w");
    init_workspace(&root).unwrap();
    let project = create_project(&root, "P", "").unwrap();
    let first = create_resource(&root, ResourceKind::Document, "first.md").unwrap();
    let second = create_resource(&root, ResourceKind::Document, "second.md").unwrap();
    let config_path = root.join(&project.path).join(MARKER);

    let mut config: ProjectConfig =
        serde_json::from_slice(&fs::read(&config_path).unwrap()).unwrap();
    config.resources.push(ResourceLink {
        kind: ResourceKind::Document,
        path: first.path.clone(),
        alias: "chosen.md".into(),
        source_path: None,
    });
    fs::write(&config_path, serde_json::to_vec_pretty(&config).unwrap()).unwrap();
    snapshot(&root).unwrap();
    let link = root.join(&project.path).join("关联文档/chosen.md");
    assert_eq!(fs::read_link(&link).unwrap(), root.join(&first.path));

    config.resources[0].path = second.path.clone();
    fs::write(&config_path, serde_json::to_vec_pretty(&config).unwrap()).unwrap();
    snapshot(&root).unwrap();
    assert_eq!(fs::read_link(&link).unwrap(), root.join(&second.path));

    config.resources.clear();
    fs::write(&config_path, serde_json::to_vec_pretty(&config).unwrap()).unwrap();
    snapshot(&root).unwrap();
    assert!(fs::symlink_metadata(link).is_err());
}

#[test]
fn reconciliation_preflights_invalid_targets_and_duplicate_aliases_without_partial_changes() {
    let td = temp_root();
    let root = td.path().join("w");
    init_workspace(&root).unwrap();
    let project = create_project(&root, "P", "").unwrap();
    let good = create_resource(&root, ResourceKind::Document, "good.md").unwrap();
    let config_path = root.join(&project.path).join(MARKER);
    let stale = root.join(&project.path).join("关联文档/stale.md");
    #[cfg(unix)]
    std::os::unix::fs::symlink(root.join(&good.path), &stale).unwrap();

    let mut config: ProjectConfig =
        serde_json::from_slice(&fs::read(&config_path).unwrap()).unwrap();
    config.resources = vec![
        ResourceLink {
            kind: ResourceKind::Document,
            path: good.path.clone(),
            alias: "new.md".into(),
            source_path: None,
        },
        ResourceLink {
            kind: ResourceKind::Document,
            path: "../outside.md".into(),
            alias: "invalid.md".into(),
            source_path: None,
        },
    ];
    fs::write(&config_path, serde_json::to_vec_pretty(&config).unwrap()).unwrap();
    assert!(snapshot(&root).unwrap_err().contains("关联资源路径无效"));
    assert!(fs::symlink_metadata(&stale)
        .unwrap()
        .file_type()
        .is_symlink());
    assert!(fs::symlink_metadata(root.join(&project.path).join("关联文档/new.md")).is_err());

    config.resources = vec![
        ResourceLink {
            kind: ResourceKind::Document,
            path: good.path.clone(),
            alias: "same.md".into(),
            source_path: None,
        },
        ResourceLink {
            kind: ResourceKind::Document,
            path: good.path.clone(),
            alias: "same.md".into(),
            source_path: None,
        },
    ];
    fs::write(&config_path, serde_json::to_vec_pretty(&config).unwrap()).unwrap();
    assert!(snapshot(&root).unwrap_err().contains("重复关联别名"));
    assert!(fs::symlink_metadata(root.join(&project.path).join("关联文档/same.md")).is_err());
}

#[test]
fn reconciliation_never_removes_an_unclassifiable_link_or_overwrites_plain_files() {
    let td = temp_root();
    let root = td.path().join("w");
    init_workspace(&root).unwrap();
    let project = create_project(&root, "P", "").unwrap();
    let resource = create_resource(&root, ResourceKind::Document, "doc.md").unwrap();
    let config_path = root.join(&project.path).join(MARKER);
    let mut config: ProjectConfig =
        serde_json::from_slice(&fs::read(&config_path).unwrap()).unwrap();
    config.resources.push(ResourceLink {
        kind: ResourceKind::Document,
        path: resource.path.clone(),
        alias: "blocked.md".into(),
        source_path: None,
    });
    fs::write(&config_path, serde_json::to_vec_pretty(&config).unwrap()).unwrap();
    let blocked = root.join(&project.path).join("关联文档/blocked.md");
    fs::write(&blocked, "keep me").unwrap();
    assert!(snapshot(&root).unwrap_err().contains("普通文件或目录占用"));
    assert_eq!(fs::read_to_string(&blocked).unwrap(), "keep me");

    fs::remove_file(&blocked).unwrap();
    #[cfg(unix)]
    {
        let outside = td.path().join("outside.md");
        fs::write(&outside, "outside").unwrap();
        std::os::unix::fs::symlink(&outside, &blocked).unwrap();
        assert!(snapshot(&root)
            .unwrap_err()
            .contains("无法确认关联软链接归属"));
        assert!(fs::symlink_metadata(&blocked)
            .unwrap()
            .file_type()
            .is_symlink());
        assert_eq!(fs::read_to_string(outside).unwrap(), "outside");
    }
}

#[test]
fn project_lookup_ignores_project_directory_symlinks() {
    let td = temp_root();
    let root = td.path().join("w");
    init_workspace(&root).unwrap();
    let outside = td.path().join("outside-project");
    fs::create_dir(&outside).unwrap();
    #[cfg(unix)]
    std::os::unix::fs::symlink(&outside, root.join("项目/linked-project")).unwrap();
    #[cfg(unix)]
    assert!(project_dir(&root, "any-id").is_err());
    #[cfg(unix)]
    assert!(snapshot(&root).unwrap().projects.is_empty());
}

#[test]
fn project_lookup_skips_unmarked_and_marker_symlink_directories() {
    let td = temp_root();
    let root = td.path().join("w");
    init_workspace(&root).unwrap();
    let first = create_project(&root, "First", "").unwrap();
    let second = create_project(&root, "Second", "").unwrap();

    let unmarked = root.join("项目/00-unmarked");
    fs::create_dir(&unmarked).unwrap();
    fs::write(unmarked.join("keep.txt"), "leave alone").unwrap();

    #[cfg(unix)]
    {
        let linked_marker = root.join("项目/01-linked-marker");
        fs::create_dir(&linked_marker).unwrap();
        std::os::unix::fs::symlink(
            root.join(&first.path).join(MARKER),
            linked_marker.join(MARKER),
        )
        .unwrap();
    }

    assert_eq!(
        project_dir(&root, &second.id).unwrap(),
        root.join(&second.path)
    );
    let projects = snapshot(&root).unwrap().projects;
    assert_eq!(projects.len(), 2);
    assert!(projects.iter().any(|project| project.id == first.id));
    assert!(projects.iter().any(|project| project.id == second.id));
    assert_eq!(
        fs::read_to_string(unmarked.join("keep.txt")).unwrap(),
        "leave alone"
    );
}

#[test]
fn corrupt_project_json_error_identifies_its_config_path_without_changing_other_projects() {
    let td = temp_root();
    let root = td.path().join("w");
    init_workspace(&root).unwrap();
    let good = create_project(&root, "A-good", "").unwrap();
    let broken = create_project(&root, "B-broken", "").unwrap();
    let good_config = root.join(&good.path).join(MARKER);
    let good_bytes = fs::read(&good_config).unwrap();
    let sentinel = root.join(&good.path).join("untouched.txt");
    fs::write(&sentinel, "preserve").unwrap();

    let broken_config = root.join(&broken.path).join(MARKER);
    fs::write(&broken_config, "{invalid json").unwrap();
    let error = project_dir(&root, &good.id).unwrap_err();
    assert!(error.contains("项目配置损坏"));
    assert!(error.contains(&broken_config.display().to_string()));
    assert_eq!(fs::read(&good_config).unwrap(), good_bytes);
    assert_eq!(fs::read_to_string(&sentinel).unwrap(), "preserve");

    let snapshot_error = snapshot(&root).unwrap_err();
    assert!(snapshot_error.contains(&broken_config.display().to_string()));
    assert_eq!(fs::read(&good_config).unwrap(), good_bytes);
    assert_eq!(fs::read_to_string(&sentinel).unwrap(), "preserve");

    fs::write(
        &broken_config,
        serde_json::to_vec_pretty(&serde_json::json!({
            "version": CONFIG_VERSION + 1,
            "id": broken.id,
            "name": broken.name,
            "description": "",
            "resources": []
        }))
        .unwrap(),
    )
    .unwrap();
    let version_error = project_dir(&root, &good.id).unwrap_err();
    assert!(version_error.contains("不支持的项目配置版本"));
    assert!(version_error.contains(&broken_config.display().to_string()));
    assert_eq!(fs::read(&good_config).unwrap(), good_bytes);
    assert_eq!(fs::read_to_string(&sentinel).unwrap(), "preserve");
}

#[test]
fn missing_association_error_names_project_and_resource_without_partial_link_changes() {
    let td = temp_root();
    let root = td.path().join("w");
    init_workspace(&root).unwrap();
    let good = create_project(&root, "A-good", "").unwrap();
    let broken = create_project(&root, "Z-missing", "").unwrap();
    let resource = create_resource(&root, ResourceKind::Document, "valid.md").unwrap();
    fs::write(root.join(&resource.path), "preserve resource").unwrap();

    let good_config_path = root.join(&good.path).join(MARKER);
    let mut good_config: ProjectConfig =
        serde_json::from_slice(&fs::read(&good_config_path).unwrap()).unwrap();
    good_config.resources.push(ResourceLink {
        kind: ResourceKind::Document,
        path: resource.path.clone(),
        alias: "generated.md".into(),
        source_path: None,
    });
    fs::write(
        &good_config_path,
        serde_json::to_vec_pretty(&good_config).unwrap(),
    )
    .unwrap();
    let good_config_bytes = fs::read(&good_config_path).unwrap();

    let missing_path = "文档/absent.md";
    let broken_config_path = root.join(&broken.path).join(MARKER);
    let mut broken_config: ProjectConfig =
        serde_json::from_slice(&fs::read(&broken_config_path).unwrap()).unwrap();
    broken_config.resources.push(ResourceLink {
        kind: ResourceKind::Document,
        path: missing_path.into(),
        alias: "absent.md".into(),
        source_path: None,
    });
    fs::write(
        &broken_config_path,
        serde_json::to_vec_pretty(&broken_config).unwrap(),
    )
    .unwrap();
    let broken_config_bytes = fs::read(&broken_config_path).unwrap();

    let error = snapshot(&root).unwrap_err();
    assert!(error.contains("项目 Z-missing"));
    assert!(error.contains(missing_path));
    assert!(!root.join(&good.path).join("关联文档/generated.md").exists());
    assert_eq!(fs::read(&good_config_path).unwrap(), good_config_bytes);
    assert_eq!(fs::read(&broken_config_path).unwrap(), broken_config_bytes);
    assert_eq!(
        fs::read_to_string(root.join(&resource.path)).unwrap(),
        "preserve resource"
    );
}

#[test]
fn snapshot_rejects_duplicate_project_uuids_before_changing_links() {
    let td = temp_root();
    let root = td.path().join("w");
    init_workspace(&root).unwrap();
    let first = create_project(&root, "First", "").unwrap();
    let second = create_project(&root, "Second", "").unwrap();
    let resource = create_resource(&root, ResourceKind::Document, "doc.md").unwrap();
    fs::write(root.join(&resource.path), "resource data").unwrap();

    let first_config_path = root.join(&first.path).join(MARKER);
    let mut first_config: ProjectConfig =
        serde_json::from_slice(&fs::read(&first_config_path).unwrap()).unwrap();
    first_config.resources.push(ResourceLink {
        kind: ResourceKind::Document,
        path: resource.path.clone(),
        alias: "pending.md".into(),
        source_path: None,
    });
    fs::write(
        &first_config_path,
        serde_json::to_vec_pretty(&first_config).unwrap(),
    )
    .unwrap();

    let second_config_path = root.join(&second.path).join(MARKER);
    let mut second_config: ProjectConfig =
        serde_json::from_slice(&fs::read(&second_config_path).unwrap()).unwrap();
    second_config.id = first.id.clone();
    fs::write(
        &second_config_path,
        serde_json::to_vec_pretty(&second_config).unwrap(),
    )
    .unwrap();
    let first_bytes = fs::read(&first_config_path).unwrap();
    let second_bytes = fs::read(&second_config_path).unwrap();

    let error = snapshot(&root).unwrap_err();
    assert!(error.contains("项目 UUID 重复"));
    assert!(error.contains(&first_config_path.display().to_string()));
    assert!(error.contains(&second_config_path.display().to_string()));
    assert!(!root.join(&first.path).join("关联文档/pending.md").exists());
    assert_eq!(fs::read(&first_config_path).unwrap(), first_bytes);
    assert_eq!(fs::read(&second_config_path).unwrap(), second_bytes);
    assert_eq!(
        fs::read_to_string(root.join(&resource.path)).unwrap(),
        "resource data"
    );
}

#[test]
fn snapshot_rejects_duplicate_temporary_uuids_before_changing_links() {
    let td = temp_root();
    let root = td.path().join("w");
    init_workspace(&root).unwrap();
    let project = create_project(&root, "Project", "").unwrap();
    let resource = create_resource(&root, ResourceKind::Document, "doc.md").unwrap();
    fs::write(root.join(&resource.path), "resource data").unwrap();
    let project_config_path = root.join(&project.path).join(MARKER);
    let mut project_config: ProjectConfig =
        serde_json::from_slice(&fs::read(&project_config_path).unwrap()).unwrap();
    project_config.resources.push(ResourceLink {
        kind: ResourceKind::Document,
        path: resource.path.clone(),
        alias: "pending.md".into(),
        source_path: None,
    });
    fs::write(
        &project_config_path,
        serde_json::to_vec_pretty(&project_config).unwrap(),
    )
    .unwrap();

    let first = create_temporary(&root, "TempOne").unwrap();
    let second = create_temporary(&root, "TempTwo").unwrap();
    let first_marker = root.join(&first.path).join(TEMP_MARKER);
    let second_marker = root.join(&second.path).join(TEMP_MARKER);
    let mut second_config: TemporaryMarker =
        serde_json::from_slice(&fs::read(&second_marker).unwrap()).unwrap();
    second_config.id = first.id.clone();
    fs::write(
        &second_marker,
        serde_json::to_vec_pretty(&second_config).unwrap(),
    )
    .unwrap();

    let project_bytes = fs::read(&project_config_path).unwrap();
    let first_bytes = fs::read(&first_marker).unwrap();
    let second_bytes = fs::read(&second_marker).unwrap();
    let error = snapshot(&root).unwrap_err();
    assert!(error.contains("临时目录 UUID 重复"));
    assert!(error.contains(&first_marker.display().to_string()));
    assert!(error.contains(&second_marker.display().to_string()));
    assert!(!root
        .join(&project.path)
        .join("关联文档/pending.md")
        .exists());
    assert_eq!(fs::read(&project_config_path).unwrap(), project_bytes);
    assert_eq!(fs::read(&first_marker).unwrap(), first_bytes);
    assert_eq!(fs::read(&second_marker).unwrap(), second_bytes);
    assert_eq!(
        fs::read_to_string(root.join(&resource.path)).unwrap(),
        "resource data"
    );
}

#[test]
fn snapshot_rejects_invalid_project_uuid_with_config_path_before_reconciliation() {
    let td = temp_root();
    let root = td.path().join("w");
    init_workspace(&root).unwrap();
    let first = create_project(&root, "A-first", "").unwrap();
    let invalid = create_project(&root, "Z-invalid", "").unwrap();
    let resource = create_resource(&root, ResourceKind::Document, "doc.md").unwrap();
    fs::write(root.join(&resource.path), "resource data").unwrap();

    let first_config_path = root.join(&first.path).join(MARKER);
    let mut first_config: ProjectConfig =
        serde_json::from_slice(&fs::read(&first_config_path).unwrap()).unwrap();
    first_config.resources.push(ResourceLink {
        kind: ResourceKind::Document,
        path: resource.path.clone(),
        alias: "pending.md".into(),
        source_path: None,
    });
    fs::write(
        &first_config_path,
        serde_json::to_vec_pretty(&first_config).unwrap(),
    )
    .unwrap();

    let invalid_config_path = root.join(&invalid.path).join(MARKER);
    let mut invalid_config: ProjectConfig =
        serde_json::from_slice(&fs::read(&invalid_config_path).unwrap()).unwrap();
    invalid_config.id = "not-a-uuid".into();
    fs::write(
        &invalid_config_path,
        serde_json::to_vec_pretty(&invalid_config).unwrap(),
    )
    .unwrap();
    let first_bytes = fs::read(&first_config_path).unwrap();
    let invalid_bytes = fs::read(&invalid_config_path).unwrap();

    let error = snapshot(&root).unwrap_err();
    assert!(error.contains("项目 UUID 无效"));
    assert!(error.contains(&invalid_config_path.display().to_string()));
    assert!(!root.join(&first.path).join("关联文档/pending.md").exists());
    assert_eq!(fs::read(&first_config_path).unwrap(), first_bytes);
    assert_eq!(fs::read(&invalid_config_path).unwrap(), invalid_bytes);
    assert_eq!(
        fs::read_to_string(root.join(&resource.path)).unwrap(),
        "resource data"
    );
}

#[test]
fn project_agents_file_is_created_once_and_preserved_by_later_operations() {
    let td = temp_root();
    let root = td.path().join("w");
    init_workspace(&root).unwrap();
    let project = create_project(&root, "P", "initial").unwrap();
    let agents = root.join(&project.path).join("AGENTS.md");
    let initial = fs::read_to_string(&agents).unwrap();
    assert!(initial.contains(".personal-workbench.json"));
    assert!(initial.contains("关联仓库`、`关联文档`、`关联附件`、`关联脚本"));
    assert!(!initial.contains("编码规范"));

    fs::write(&agents, "user customization\n").unwrap();
    let renamed = rename_project(&root, &project.id, &project.revision, "Renamed").unwrap();
    update_project(&root, &renamed.id, &renamed.revision, "Renamed", "updated").unwrap();
    assert_eq!(
        fs::read_to_string(root.join(&renamed.path).join("AGENTS.md")).unwrap(),
        "user customization\n"
    );
}

#[test]
fn project_repository_and_script_text_can_be_read_and_saved_safely() {
    let td = temp_root();
    let root = td.path().join("w");
    init_workspace(&root).unwrap();
    let project = create_project(&root, "P", "").unwrap();
    let project_file = root.join(&project.path).join("src/main.rs");
    fs::create_dir_all(project_file.parent().unwrap()).unwrap();
    fs::write(&project_file, "fn project() {}\n").unwrap();
    let project_relative = format!("{}/src/main.rs", project.path);
    let project_text = read_text(&root, &project_relative).unwrap();
    write_text(
        &root,
        &project_relative,
        "fn project() { /* edited */ }\n",
        Some(&project_text.revision),
    )
    .unwrap();

    let repository = create_resource(&root, ResourceKind::Repository, "repo").unwrap();
    let source = root.join(&repository.path).join("src/main.rs");
    fs::create_dir_all(source.parent().unwrap()).unwrap();
    fs::write(&source, "fn repo() {}\n").unwrap();
    let project = link_resource(
        &root,
        &project.id,
        &project.revision,
        ResourceKind::Repository,
        &repository.path,
        None,
    )
    .unwrap();
    let linked_source = format!("{}/关联仓库/repo/src/main.rs", project.path);
    let source_text = read_text(&root, &linked_source).unwrap();
    write_text(
        &root,
        &linked_source,
        "fn repo() { /* edited */ }\n",
        Some(&source_text.revision),
    )
    .unwrap();
    assert!(fs::read_to_string(&source).unwrap().contains("edited"));
    assert_eq!(
        resolve_workspace_path(&root, &format!("{}/关联仓库/repo/src", project.path)).unwrap(),
        fs::canonicalize(root.join(&repository.path).join("src")).unwrap()
    );
    assert_eq!(
        resolve_workspace_path(&root, "").unwrap(),
        fs::canonicalize(&root).unwrap()
    );

    let script = create_resource(&root, ResourceKind::Script, "build.sh").unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(root.join(&script.path), fs::Permissions::from_mode(0o755)).unwrap();
    }
    let revision = read_text(&root, &script.path).unwrap().revision;
    write_text(&root, &script.path, "#!/bin/sh\necho ok\n", Some(&revision)).unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(
            fs::metadata(root.join(&script.path))
                .unwrap()
                .permissions()
                .mode()
                & 0o777,
            0o755
        );
    }
    assert!(write_text(&root, &repository.path, "invalid", Some("revision")).is_err());
    let git_config = root.join(&repository.path).join(".git/config");
    fs::create_dir_all(git_config.parent().unwrap()).unwrap();
    fs::write(&git_config, "keep git metadata").unwrap();
    assert!(write_text(&root, "仓库/repo/.git/config", "damage", Some("revision")).is_err());
    assert_eq!(
        fs::read_to_string(&git_config).unwrap(),
        "keep git metadata"
    );

    #[cfg(unix)]
    {
        use std::os::unix::fs::symlink;
        let unregistered = root.join(&project.path).join("unregistered");
        symlink(root.join(&repository.path), &unregistered).unwrap();
        assert!(read_text(&root, &format!("{}/unregistered/src/main.rs", project.path)).is_err());
        assert!(
            resolve_workspace_path(&root, &format!("{}/unregistered/src", project.path)).is_err()
        );
    }
}

#[test]
fn rename_failure_preserves_external_config_link_and_old_path_occupant() {
    let td = temp_root();
    let root = td.path().join("w");
    init_workspace(&root).unwrap();
    let first = create_project(&root, "P1", "").unwrap();
    let second = create_project(&root, "P2", "").unwrap();
    let resource = create_resource(&root, ResourceKind::Document, "old.md").unwrap();
    let other = create_resource(&root, ResourceKind::Document, "other.md").unwrap();
    let first = link_resource(
        &root,
        &first.id,
        &first.revision,
        resource.kind,
        &resource.path,
        None,
    )
    .unwrap();
    link_resource(
        &root,
        &second.id,
        &second.revision,
        resource.kind,
        &resource.path,
        None,
    )
    .unwrap();

    let result = rename_resource_with_hook(
        &root,
        resource.kind,
        &resource.path,
        "new.md",
        |index, project_dir, _| {
            if index == 0 {
                let config_path = project_dir.join(MARKER);
                let mut config: ProjectConfig =
                    serde_json::from_slice(&fs::read(&config_path).unwrap()).unwrap();
                config.description = "external edit during rename".into();
                let external_config = serde_json::to_vec_pretty(&config).unwrap();
                fs::write(&config_path, &external_config).unwrap();
                let new_link = project_dir.join("关联文档/new.md");
                fs::remove_file(&new_link).unwrap();
                #[cfg(unix)]
                std::os::unix::fs::symlink(root.join(&other.path), &new_link).unwrap();
                fs::write(root.join(&resource.path), "external occupant").unwrap();
                Err("injected failure after external edits".into())
            } else {
                Ok(())
            }
        },
    );

    let error = result.unwrap_err();
    assert!(error.contains("需人工处理"));
    let config_path = root.join(&first.path).join(MARKER);
    let config: ProjectConfig = serde_json::from_slice(&fs::read(&config_path).unwrap()).unwrap();
    assert_eq!(config.description, "external edit during rename");
    assert_eq!(config.resources[0].path, "文档/new.md");
    #[cfg(unix)]
    assert_eq!(
        fs::read_link(root.join(&first.path).join("关联文档/new.md")).unwrap(),
        root.join(&other.path)
    );
    assert_eq!(
        fs::read_to_string(root.join(&resource.path)).unwrap(),
        "external occupant"
    );
    assert!(root.join("文档/new.md").exists());
}

#[test]
fn rename_rollback_keeps_replaced_link_and_restores_only_matching_objects() {
    let td = temp_root();
    let root = td.path().join("w");
    init_workspace(&root).unwrap();
    let project = create_project(&root, "P", "").unwrap();
    let resource = create_resource(&root, ResourceKind::Document, "old.md").unwrap();
    let other = create_resource(&root, ResourceKind::Document, "other.md").unwrap();
    let project = link_resource(
        &root,
        &project.id,
        &project.revision,
        resource.kind,
        &resource.path,
        None,
    )
    .unwrap();

    let result = rename_resource_with_hook(
        &root,
        resource.kind,
        &resource.path,
        "new.md",
        |_, project_dir, _| {
            let new_link = project_dir.join("关联文档/new.md");
            fs::remove_file(&new_link).unwrap();
            #[cfg(unix)]
            std::os::unix::fs::symlink(root.join(&other.path), &new_link).unwrap();
            Err("injected failure after external link replacement".into())
        },
    );
    let error = result.unwrap_err();
    assert!(error.contains("需人工处理"));
    let old_link = root.join(&project.path).join("关联文档/old.md");
    let new_link = root.join(&project.path).join("关联文档/new.md");
    #[cfg(unix)]
    {
        assert_eq!(fs::read_link(&old_link).unwrap(), root.join(&resource.path));
        assert_eq!(fs::read_link(&new_link).unwrap(), root.join(&other.path));
    }
    assert!(root.join(&resource.path).exists());
    assert!(!root.join("文档/new.md").exists());
    assert_eq!(
        snapshot(&root).unwrap().projects[0].resources[0].path,
        resource.path
    );
}

#[test]
fn rename_rollback_does_not_replace_a_new_object_at_the_old_resource_path() {
    let td = temp_root();
    let root = td.path().join("w");
    init_workspace(&root).unwrap();
    let project = create_project(&root, "P", "").unwrap();
    let resource = create_resource(&root, ResourceKind::Document, "old.md").unwrap();
    let project = link_resource(
        &root,
        &project.id,
        &project.revision,
        resource.kind,
        &resource.path,
        None,
    )
    .unwrap();

    let result =
        rename_resource_with_hook(&root, resource.kind, &resource.path, "new.md", |_, _, _| {
            fs::write(root.join(&resource.path), "created by another process").unwrap();
            Err("injected failure after old path reuse".into())
        });
    let error = result.unwrap_err();
    assert!(error.contains("需人工处理"));
    assert!(error.contains("旧资源路径已被外部占用"));
    assert_eq!(
        fs::read_to_string(root.join(&resource.path)).unwrap(),
        "created by another process"
    );
    assert!(root.join("文档/new.md").exists());
    assert_eq!(
        fs::read_link(root.join(&project.path).join("关联文档/old.md")).unwrap(),
        root.join(&resource.path)
    );
    assert_eq!(
        snapshot(&root).unwrap().projects[0].resources[0].path,
        resource.path
    );
}

#[test]
fn interrupted_project_creation_preserves_concurrent_unknown_content() {
    let td = temp_root();
    let root = td.path().join("w");
    init_workspace(&root).unwrap();
    let result = create_project_with_hook(&root, "Partial", "", |_, association_dir| {
        fs::write(association_dir.join("external.txt"), "keep this").unwrap();
        Err("injected creation failure".into())
    });
    assert!(result.unwrap_err().contains("项目创建中断"));
    let external = root.join("项目/Partial/关联仓库/external.txt");
    assert_eq!(fs::read_to_string(external).unwrap(), "keep this");
    assert!(!root.join("项目/Partial/关联文档").exists());
    assert!(snapshot(&root).unwrap().projects.is_empty());
}

#[test]
fn project_rename_failure_does_not_replace_an_externally_created_old_directory() {
    let td = temp_root();
    let root = td.path().join("w");
    init_workspace(&root).unwrap();
    let project = create_project(&root, "Original", "before").unwrap();
    let old_path = root.join(&project.path);
    let result = update_project_with_hook(
        &root,
        &project.id,
        &project.revision,
        "Renamed",
        "after",
        |original, current| {
            let config_path = current.join(MARKER);
            let mut config: ProjectConfig =
                serde_json::from_slice(&fs::read(&config_path).unwrap()).unwrap();
            config.description = "external edit".into();
            fs::write(&config_path, serde_json::to_vec_pretty(&config).unwrap()).unwrap();
            fs::create_dir(original).unwrap();
            Ok(())
        },
    );
    let error = result.unwrap_err();
    assert!(error.contains("需人工处理"));
    assert!(error.contains("原项目路径已被外部占用"));
    assert!(old_path.is_dir());
    let renamed_path = root.join("项目/Renamed");
    assert!(renamed_path.join(MARKER).is_file());
    let config: ProjectConfig =
        serde_json::from_slice(&fs::read(renamed_path.join(MARKER)).unwrap()).unwrap();
    assert_eq!(config.description, "external edit");
}

#[test]
fn failed_link_save_preserves_a_replaced_link_instead_of_removing_it() {
    let td = temp_root();
    let root = td.path().join("w");
    init_workspace(&root).unwrap();
    let project = create_project(&root, "P", "before").unwrap();
    let target = create_resource(&root, ResourceKind::Document, "target.md").unwrap();
    let other = create_resource(&root, ResourceKind::Document, "other.md").unwrap();
    let result = link_resource_with_hook(
        &root,
        &project.id,
        &project.revision,
        target.kind,
        &target.path,
        None,
        |project_dir, link_path| {
            let config_path = project_dir.join(MARKER);
            let mut config: ProjectConfig =
                serde_json::from_slice(&fs::read(&config_path).unwrap()).unwrap();
            config.description = "external edit".into();
            fs::write(&config_path, serde_json::to_vec_pretty(&config).unwrap()).unwrap();
            fs::remove_file(link_path).unwrap();
            #[cfg(unix)]
            std::os::unix::fs::symlink(root.join(&other.path), link_path).unwrap();
            Ok(())
        },
    );
    let error = result.unwrap_err();
    assert!(error.contains("保留被外部替换的路径"));
    let config: ProjectConfig =
        serde_json::from_slice(&fs::read(root.join(&project.path).join(MARKER)).unwrap()).unwrap();
    assert_eq!(config.description, "external edit");
    let link = root.join(&project.path).join("关联文档/target.md");
    #[cfg(unix)]
    assert_eq!(fs::read_link(link).unwrap(), root.join(&other.path));
}
