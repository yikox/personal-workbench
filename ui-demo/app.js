(() => {
  "use strict";

  const iconPaths = {
    home: '<path d="m3 10 9-7 9 7v10a1 1 0 0 1-1 1h-5v-6H9v6H4a1 1 0 0 1-1-1z"/>',
    folder: '<path d="M3 6.5A1.5 1.5 0 0 1 4.5 5H10l2 2h7.5A1.5 1.5 0 0 1 21 8.5v9a1.5 1.5 0 0 1-1.5 1.5h-15A1.5 1.5 0 0 1 3 17.5z"/><path d="M3 9h18"/>',
    stack: '<path d="m12 3 9 5-9 5-9-5z"/><path d="m3 12 9 5 9-5M3 16l9 5 9-5"/>',
    spark: '<path d="m12 3 1.9 5.8L20 11l-6.1 2.2L12 19l-1.9-5.8L4 11l6.1-2.2z"/><path d="m19 14 .8 2.2L22 17l-2.2.8L19 20l-.8-2.2L16 17l2.2-.8z"/>',
    plus: '<path d="M12 5v14M5 12h14"/>',
    more: '<circle cx="5" cy="12" r="1"/><circle cx="12" cy="12" r="1"/><circle cx="19" cy="12" r="1"/>',
    shield: '<path d="M12 3 20 6v5c0 5-3.4 8.3-8 10-4.6-1.7-8-5-8-10V6z"/><path d="m9 12 2 2 4-4"/>',
    search: '<circle cx="10.8" cy="10.8" r="6.3"/><path d="m16 16 4.2 4.2"/>',
    help: '<circle cx="12" cy="12" r="9"/><path d="M9.7 9a2.4 2.4 0 1 1 4.3 1.5c-.7.9-2 1.2-2 2.7M12 16.8v.1"/>',
    chevron: '<path d="m8 10 4 4 4-4"/>',
    arrow: '<path d="M5 12h14M13 6l6 6-6 6"/>',
    external: '<path d="M14 4h6v6M20 4l-9 9"/><path d="M18 13v5a2 2 0 0 1-2 2H6a2 2 0 0 1-2-2V8a2 2 0 0 1 2-2h5"/>',
    clock: '<circle cx="12" cy="12" r="9"/><path d="M12 7v5l3 2"/>',
    branch: '<circle cx="6" cy="6" r="2"/><circle cx="18" cy="18" r="2"/><path d="M6 8v4a6 6 0 0 0 6 6h4M18 16v-4a6 6 0 0 0-6-6H8"/>',
    file: '<path d="M6 3h8l4 4v14H6z"/><path d="M14 3v5h5M9 13h6M9 17h6"/>',
    code: '<path d="m8 8-4 4 4 4M16 8l4 4-4 4M14 5l-4 14"/>',
    terminal: '<rect x="3" y="4" width="18" height="16" rx="2"/><path d="m7 9 3 3-3 3M13 15h4"/>',
    app: '<rect x="4" y="4" width="16" height="16" rx="3"/><path d="M8 8h.1M12 8h.1M16 8h.1M8 12h8M8 16h5"/>',
    book: '<path d="M4 5.5A2.5 2.5 0 0 1 6.5 3H20v16H6.5A2.5 2.5 0 0 0 4 21z"/><path d="M4 5v16M8 7h8M8 11h8"/>',
    edit: '<path d="m4 16.5-.8 4.3 4.3-.8L19 8.5 15.5 5z"/><path d="m13.8 6.7 3.5 3.5"/>',
    trash: '<path d="M4 7h16M10 11v6M14 11v6M6 7l1 14h10l1-14M9 7V4h6v3"/>',
    dots: '<circle cx="12" cy="5" r=".8"/><circle cx="12" cy="12" r=".8"/><circle cx="12" cy="19" r=".8"/>',
    check: '<path d="m5 12 4 4L19 6"/>',
    pin: '<path d="m16 3 5 5-4 1-4 4-1 5-2-2-5 5-1-1 5-5-2-2 5-1 4-4z"/>',
    close: '<path d="m6 6 12 12M18 6 6 18"/>',
    refresh: '<path d="M20 7v5h-5M4 17v-5h5"/><path d="M5.5 9a7 7 0 0 1 11.6-2L20 12M4 12l2.9 5a7 7 0 0 0 11.6-2"/>',
    calendar: '<rect x="3" y="5" width="18" height="16" rx="2"/><path d="M16 3v4M8 3v4M3 10h18"/>',
    arrowUp: '<path d="M12 19V5M6 11l6-6 6 6"/>',
    database: '<ellipse cx="12" cy="5" rx="8" ry="3"/><path d="M4 5v7c0 1.7 3.6 3 8 3s8-1.3 8-3V5M4 12v7c0 1.7 3.6 3 8 3s8-1.3 8-3v-7"/>',
    message: '<path d="M20 11.5a7.5 7.5 0 0 1-8 7.5 8 8 0 0 1-3.5-.8L4 20l1.3-3.6A7.2 7.2 0 0 1 4 12c0-4.1 3.6-7.5 8-7.5s8 3.1 8 7z"/><path d="M8 12h.1M12 12h.1M16 12h.1"/>',
    image: '<rect x="3" y="4" width="18" height="16" rx="2"/><circle cx="8.5" cy="9" r="1.5"/><path d="m21 15-5-5L5 20"/>',
    settings: '<circle cx="12" cy="12" r="3"/><path d="m19.4 15 .1.1 1.4 1.1-1.4 2.4-1.7-.6a7.6 7.6 0 0 1-1.5.9l-.3 1.8h-2.8l-.3-1.8a7.6 7.6 0 0 1-1.5-.9l-1.7.6-1.4-2.4 1.4-1.1a7.6 7.6 0 0 1 0-1.8l-1.4-1.1 1.4-2.4 1.7.6a7.6 7.6 0 0 1 1.5-.9l.3-1.8h2.8l.3 1.8a7.6 7.6 0 0 1 1.5.9l1.7-.6 1.4 2.4-1.4 1.1a7.6 7.6 0 0 1 0 1.8z"/>',
  };

  const projects = [
    { id: "studio", name: "栖居计划", monogram: "栖", color: "#e7e1f4", ink: "#66569e", description: "为日常留一间安静的数字书房。", updated: "今天更新", repo: "栖居计划", docs: ["项目说明.md", "访谈摘录.md"], attachments: ["版式参考.pdf", "方案稿.docx"] },
    { id: "field", name: "田野手记", monogram: "田", color: "#e6eee7", ink: "#547b61", description: "整理城市里的小店、声音与故事。", updated: "昨天更新", repo: "field-notes", docs: ["采集计划.md"], attachments: ["巷口春天.png"] },
    { id: "garden", name: "慢慢花园", monogram: "慢", color: "#f3eadf", ink: "#9b744b", description: "一个关于季节、植物和观察的练习。", updated: "周一更新", repo: "slow-garden", docs: ["种植日志.md"], attachments: [] },
  ];

  const resources = [
    { id: "repo-studio", type: "repo", title: "栖居计划", path: "仓库 / qiju-project", detail: "main · 最近提交 2 小时前", project: "栖居计划", content: "// 栖居计划 · 示例代码\nexport const idea = {\n  name: '一间安静的数字书房',\n  focus: ['阅读', '整理', '慢下来']\n};\n\n// 文件浏览与轻量编辑示例" },
    { id: "repo-field", type: "repo", title: "田野手记", path: "仓库 / field-notes", detail: "main · 最近提交昨天", project: "田野手记", content: "# field-notes\n\n示例仓库目录\n- notes/\n- sources/\n- README.md" },
    { id: "repo-garden", type: "repo", title: "慢慢花园", path: "仓库 / slow-garden", detail: "main · 最近提交周一", project: "慢慢花园", content: "// slow-garden · 示例文件\nconst season = '春';\nconst observation = '新芽';" },
    { id: "doc-brief", type: "doc", title: "项目说明.md", path: "文档 / 栖居计划", detail: "Markdown · 今天编辑", project: "栖居计划", content: "# 栖居计划\n\n一间安静的数字书房，帮助人们整理正在阅读、正在思考的内容。\n\n## 这段时间\n- 完成首页信息层级梳理\n- 收集 5 位读者的使用感受\n- 明确第一版的内容范围\n\n## 下一步\n把阅读记录和主题笔记串起来，先验证最小的使用闭环。" },
    { id: "doc-interview", type: "doc", title: "访谈摘录.md", path: "文档 / 栖居计划", detail: "Markdown · 昨天编辑", project: "栖居计划", content: "# 访谈摘录\n\n> 想要一个地方，能让我重新找到那些读过的东西。\n\n## 共同线索\n- 不希望整理本身变成负担\n- 更习惯从项目和主题进入\n- 需要保留原文出处" },
    { id: "doc-field", type: "doc", title: "采集计划.md", path: "文档 / 田野手记", detail: "Markdown · 昨天编辑", project: "田野手记", content: "# 采集计划\n\n记录沿街观察的方式与节奏。\n\n## 本周路线\n- 石桥巷\n- 老邮局周边\n\n每次采集留下照片说明、时间和一句现场印象。" },
    { id: "doc-log", type: "doc", title: "种植日志.md", path: "文档 / 慢慢花园", detail: "Markdown · 周一编辑", project: "慢慢花园", content: "# 种植日志\n\n## 4 月 12 日\n薄荷长出了新叶，窗边的光比上周更长一些。\n\n## 观察\n记录天气、浇水与叶片变化。" },
    { id: "doc-text", type: "doc", title: "灵感便笺.txt", path: "文档 / 栖居计划", detail: "纯文本 · 上周编辑", project: "栖居计划", content: "把东西放在一个找得到的地方。\n少做分类，多留线索。\n\n关键词：安静 / 线索 / 可回到" },
    { id: "att-pdf", type: "attachment", title: "版式参考.pdf", path: "附件 / 栖居计划", detail: "PDF · 外部应用打开", project: "栖居计划", content: "这是 PDF 示例文件。实际预览需要由系统中的外部应用打开。" },
    { id: "att-word", type: "attachment", title: "方案稿.docx", path: "附件 / 栖居计划", detail: "Word · 外部应用打开", project: "栖居计划", content: "这是 Word 示例文件。实际编辑需要由系统中的外部应用打开。" },
    { id: "att-photo", type: "attachment", title: "巷口春天.png", path: "附件 / 田野手记", detail: "图片 · 示例预览", project: "田野手记", content: "一张关于巷口春日的虚构示例图片。" },
    { id: "script-review", type: "script", title: "整理周报", path: "脚本 / 自定义", detail: "命令 · 周五快捷运行", project: "个人空间", content: "# 整理周报 · 命令示例\npython3 tools/weekly_summary.py\n\n此处只展示自定义脚本入口，不会真的运行命令。" },
    { id: "script-backup", type: "script", title: "打开工作区", path: "脚本 / 自定义", detail: "快捷命令 · 最近使用", project: "个人空间", content: "# 打开工作区 · 命令示例\nopen ~/Documents/Workbench\n\n此处只展示自定义快捷命令，不会访问文件系统。" },
  ];

  const sessions = [
    { id: "s1", agent: "Codex", badge: "Cx", title: "梳理首页的阅读状态", project: "栖居计划", projectId: "studio", time: "今天 10:42", excerpt: "调整首页内容分组，并检查空状态。" },
    { id: "s2", agent: "Claude", badge: "Cl", title: "整理访谈里的共同主题", project: "栖居计划", projectId: "studio", time: "昨天 16:18", excerpt: "把几段访谈中的线索归纳成主题。" },
    { id: "s3", agent: "Pi", badge: "Pi", title: "补充巷口采集清单", project: "田野手记", projectId: "field", time: "昨天 11:06", excerpt: "增加采集地点和现场记录字段。" },
    { id: "s4", agent: "Codex", badge: "Cx", title: "记录春天的种植观察", project: "慢慢花园", projectId: "garden", time: "周一 14:27", excerpt: "为每次观察补上天气和光照情况。" },
  ];

  const shortcuts = [
    { id: "project", title: "栖居计划", note: "打开项目", icon: "folder", tint: "#f0edfa" },
    { id: "app", title: "写作应用", note: "外部应用", icon: "app", tint: "#eef3f7" },
    { id: "terminal", title: "项目终端", note: "外部终端", icon: "terminal", tint: "#eff3ef" },
    { id: "script", title: "整理周报", note: "运行脚本", icon: "code", tint: "#fbf1e7" },
    { id: "command", title: "打开工作区", note: "运行命令", icon: "arrow", tint: "#f2eff9" },
  ];

  const state = {
    page: "home", projectId: "studio", workspaceTab: "overview", category: "repo", shortcutTab: "buttons",
    selectedResource: "repo-studio", resourceMode: "preview", filter: "",
    modal: null, toastTimer: 0, draft: null, modalQuery: "",
  };

  const $ = (selector, root = document) => root.querySelector(selector);
  const $$ = (selector, root = document) => [...root.querySelectorAll(selector)];
  const currentProject = () => projects.find((project) => project.id === state.projectId) || projects[0];
  const currentResource = () => resources.find((resource) => resource.id === state.selectedResource) || resources[0];
  const icon = (name, extraClass = "") => `<svg class="${extraClass}" viewBox="0 0 24 24" aria-hidden="true">${iconPaths[name] || iconPaths.file}</svg>`;
  const projectMark = (project, sizeClass = "") => `<span class="project-mark ${sizeClass}" style="background:${project.color};color:${project.ink}">${project.monogram}</span>`;

  function renderSidebar() {
    $("#sidebar-projects").innerHTML = projects.map((project) => `
      <button class="project-link ${state.page === "project" && state.projectId === project.id ? "selected" : ""}" data-project="${project.id}" aria-label="打开项目：${project.name}">
        <span class="project-color" style="background:${project.ink}"></span><span>${project.name}</span><small>${project.docs.length + 1}</small>
      </button>`).join("");
    $$("[data-page]").forEach((button) => {
      const isResourcePage = button.dataset.page === "resources";
      const active = isResourcePage
        ? state.page === "resources" && button.dataset.category === state.category
        : button.dataset.page === state.page;
      button.classList.toggle("active", active);
    });
  }

  function setBreadcrumb() {
    const resourceNames = { repo: "Git 管理", doc: "文档管理", attachment: "附件管理" };
    const pageNames = { home: "首页", project: "项目目录", resources: resourceNames[state.category], shortcuts: state.shortcutTab === "scripts" ? "快捷按钮 / 脚本" : "快捷按钮", settings: "设置" };
    const suffix = state.page === "project" ? `<span class="crumb-sep">/</span><strong>${currentProject().name}</strong>` : "";
    $("#breadcrumbs").innerHTML = `<span>AI 工作台</span><span class="crumb-sep">/</span><strong>${pageNames[state.page]}</strong>${suffix}`;
  }

  function render() {
    renderSidebar();
    setBreadcrumb();
    const view = $("#app-view");
    view.classList.toggle("doc-notes-view", state.page === "resources" && state.category === "doc");
    if (state.page === "home") view.innerHTML = renderHome();
    if (state.page === "project") view.innerHTML = renderProject();
    if (state.page === "resources") view.innerHTML = renderResources();
    if (state.page === "shortcuts") view.innerHTML = renderShortcuts();
    if (state.page === "settings") view.innerHTML = renderSettings();
    if (state.modal) renderModal();
  }

  function pageHead(eyebrow, title, subtitle, actions = "") {
    return `<div class="page-head"><div>${eyebrow ? `<div class="eyebrow">${eyebrow}</div>` : ""}<h1 class="page-title">${title}</h1>${subtitle ? `<p class="page-subtitle">${subtitle}</p>` : ""}</div>${actions ? `<div class="head-actions">${actions}</div>` : ""}</div>`;
  }

  function sessionRow(session, className = "") {
    return `<button class="session-row ${className}" data-session="${session.id}"><span class="agent-badge">${session.badge}</span><span class="session-main"><span class="session-title">${session.title}</span><span class="session-meta">${session.agent} · ${session.project} · ${session.time}</span></span><span class="session-chevron">›</span></button>`;
  }

  function shortcutButton(shortcut, className = "") {
    return `<button class="shortcut-button ${className}" data-shortcut="${shortcut.id}"><span class="shortcut-icon" style="background:${shortcut.tint}">${icon(shortcut.icon)}</span><span><strong>${shortcut.title}</strong><small>${shortcut.note}</small></span></button>`;
  }

  function renderHome() {
    const project = projects[0];
    const recent = sessions.slice(0, 3);
    const featured = shortcuts.slice(0, 4);
    return `${pageHead("个人空间 · 星期四，11 月 5 日", "把注意力留给正在做的事", "项目、资料和常用入口，都在这里轻轻接上。", `<button class="button" data-action="new-project">${icon("plus")}新建项目</button>`)}
      <section class="home-hero" aria-label="继续最近项目">
        <div class="hero-copy"><div class="hero-kicker">${icon("spark")}最近继续</div><h2 class="hero-title">${project.name}</h2><p class="hero-note">${project.description}</p><div class="hero-meta"><span>${icon("clock")}今天 10:42 活跃</span><span>${icon("branch")}1 个关联仓库</span><span>${icon("book")}2 份文档</span></div></div>
        <div class="hero-actions"><button class="button" data-action="open-project" data-project="${project.id}">项目概览</button><button class="button primary" data-action="resume-session" data-session="s1">${icon("arrow")}继续最近会话</button></div>
      </section>
      <section class="panel home-shortcuts"><div class="panel-head"><div><h2 class="panel-title">常用快捷按钮</h2><div class="panel-subtitle">打开项目、应用，或运行自定义命令</div></div><button class="icon-button small" data-action="new-shortcut" aria-label="添加快捷按钮">${icon("plus")}</button></div><div class="panel-body shortcut-strip">${featured.map((shortcut) => shortcutButton(shortcut)).join("")}</div></section>
      <div class="home-lower">
        <section class="panel"><div class="panel-head"><div><h2 class="panel-title">最近的项目会话</h2><div class="panel-subtitle">从工作台启动 · 继续在原生程序中</div></div><button class="quiet-link" data-action="open-project" data-project="studio" data-tab="sessions">全部会话</button></div><div class="panel-body session-list">${recent.map((session) => sessionRow(session)).join("")}</div></section>
      </div>`;
  }

  function renderProject() {
    const project = currentProject();
    const relevantSessions = sessions.filter((session) => session.projectId === project.id);
    const resourceRepo = resources.find((resource) => resource.type === "repo" && resource.project === project.name);
    const projectDocs = resources.filter((resource) => resource.type === "doc" && resource.project === project.name && ["md", "txt"].some((ext) => resource.title.endsWith(ext)));
    const projectAttachments = resources.filter((resource) => resource.type === "attachment" && resource.project === project.name);
    const tabs = `<div class="workspace-tabs"><button class="workspace-tab ${state.workspaceTab === "overview" ? "active" : ""}" data-workspace-tab="overview">项目概览</button><button class="workspace-tab ${state.workspaceTab === "sessions" ? "active" : ""}" data-workspace-tab="sessions">历史会话 <span>${relevantSessions.length}</span></button></div>`;
    const overview = `<section class="panel workspace-overview"><div class="workspace-intro">${projectMark(project)}<div><h2>${project.name}</h2><p>${project.description}</p></div><span class="workspace-status"><i></i>本地项目</span></div>
      <div class="resource-group-title">关联仓库 <small>项目内为软链接入口</small></div>
      ${resourceRepo ? linkedResource(resourceRepo) : `<div class="empty-state">暂无关联仓库</div>`}
      <div class="resource-group-title">关联文档 <button class="quiet-link" data-page="resources" data-category="doc">管理文档</button></div>
      ${projectDocs.map(linkedResource).join("") || `<div class="empty-state">暂无关联文档</div>`}
      ${projectAttachments.length ? `<div class="resource-group-title">关联附件 <button class="quiet-link" data-page="resources" data-category="attachment">管理附件</button></div>${projectAttachments.map(linkedResource).join("")}` : ""}
      <div class="resource-group-title">最近会话 <button class="quiet-link" data-workspace-tab="sessions">查看全部</button></div>
      <div class="session-list">${relevantSessions.length ? relevantSessions.slice(0, 2).map((session) => sessionRow(session)).join("") : `<div class="empty-state">还没有工作台启动的会话</div>`}</div>
      </section>`;
    const history = `<section class="panel"><div class="panel-head"><div><h2 class="panel-title">${project.name} · 项目历史会话</h2><div class="panel-subtitle">会话在对应的原生 Agent 中打开和继续</div></div><span class="sample-pill">工作台启动</span></div><div class="panel-body session-list">${relevantSessions.length ? relevantSessions.map((session) => sessionRow(session)).join("") : `<div class="empty-state">此示例项目还没有会话</div>`}</div></section>`;
    const right = `<aside class="panel detail-panel"><div><div class="detail-label">项目目录</div><div class="detail-value">工作台目录 / 项目 / ${project.name}</div></div><div class="detail-line"></div><div><div class="detail-label">资源概况</div><div class="detail-stat-row"><span>关联仓库</span><strong>1</strong></div><div class="detail-stat-row"><span>关联文档</span><strong>${project.docs.length}</strong></div><div class="detail-stat-row"><span>关联附件</span><strong>${projectAttachments.length}</strong></div></div><div class="detail-line"></div><div class="agent-launch-list"><div class="detail-label">启动原生 Agent</div><button class="agent-launch" data-action="open-terminal"><span><span class="agent-badge terminal-badge">${icon("terminal")}</span>打开终端</span>${icon("external")}</button>${["Codex", "Claude", "Pi"].map((agent) => `<button class="agent-launch" data-agent="${agent}"><span><span class="agent-badge">${agent === "Codex" ? "Cx" : agent === "Claude" ? "Cl" : "Pi"}</span>${agent}</span>${icon("external")}</button>`).join("")}</div></aside>`;
    return `${pageHead("", project.name, project.description, `<button class="button" data-action="project-settings">${icon("dots")}项目选项</button>`)}
      <div class="workspace-grid"><div class="workspace-main">${tabs}${state.workspaceTab === "overview" ? overview : history}</div>${right}</div>`;
  }

  function linkedResource(resource) {
    const kind = resource.type === "repo" ? "repo" : resource.type === "script" ? "script" : resource.type === "attachment" ? "attachment" : "doc";
    const glyph = resource.type === "repo" ? "code" : resource.type === "script" ? "terminal" : resource.type === "attachment" ? "image" : "file";
    return `<button class="linked-resource" data-action="open-resource" data-resource="${resource.id}"><span class="file-icon ${kind}">${icon(glyph)}</span><span class="linked-copy"><strong>${resource.title}</strong><small>${resource.path} · ${resource.detail}</small></span><span class="linked-arrow">${icon("arrow")}</span></button>`;
  }

  function navigateToResource(resource) {
    if (!resource) return;
    state.selectedResource = resource.id;
    state.category = resource.type;
    state.filter = "";
    state.resourceMode = "preview";
    state.draft = null;
    if (resource.type === "script") {
      state.page = "shortcuts";
      state.shortcutTab = "scripts";
    } else {
      state.page = "resources";
    }
    render();
  }

  function filteredResources(category = state.category) {
    const query = state.filter.trim().toLowerCase();
    return resources.filter((resource) => resource.type === category && (!query || `${resource.title} ${resource.path} ${resource.project}`.toLowerCase().includes(query)));
  }

  function resourceListItem(resource) {
    const glyph = resource.type === "repo" ? "code" : resource.type === "script" ? "terminal" : resource.type === "attachment" ? "image" : "file";
    const typeClass = resource.type === "repo" ? "repo" : resource.type === "script" ? "script" : resource.type === "attachment" ? "attachment" : "doc";
    const association = resource.type === "repo" ? ` · 关联项目：${resource.project}` : "";
    return `<button class="resource-item ${resource.id === state.selectedResource ? "selected" : ""}" data-resource="${resource.id}"><span class="file-icon ${typeClass}">${icon(glyph)}</span><span class="linked-copy"><strong>${resource.title}</strong><small>${resource.path} · ${resource.detail}${association}</small></span></button>`;
  }

  function renderMarkdown(markdown) {
    const escaped = markdown.replaceAll("&", "&amp;").replaceAll("<", "&lt;").replaceAll(">", "&gt;");
    return escaped.split("\n").map((line) => {
      if (line.startsWith("# ")) return `<h3>${line.slice(2)}</h3>`;
      if (line.startsWith("## ")) return `<h4>${line.slice(3)}</h4>`;
      if (line.startsWith("### ")) return `<h4>${line.slice(4)}</h4>`;
      if (line.startsWith("> ")) return `<p><em>${line.slice(2)}</em></p>`;
      if (line.startsWith("- ")) return `<p>· ${line.slice(2)}</p>`;
      if (line.trim()) return `<p>${line.replace(/`([^`]+)`/g, "<code>$1</code>")}</p>`;
      return "";
    }).join("");
  }

  function renderResourceDetail(category = state.category) {
    const resource = currentResource();
    if (!resource || resource.type !== category) return `<div class="panel resource-detail"><div class="empty-state">选择一项示例内容</div></div>`;
    const isExternalDoc = resource.type === "attachment" && (resource.title.endsWith(".pdf") || resource.title.endsWith(".docx"));
    const isImage = resource.type === "attachment" && resource.title.endsWith(".png");
    const value = state.draft !== null && resource.id === state.selectedResource ? state.draft : resource.content;
    const actions = `${resource.type === "repo" ? `<button class="icon-button" data-action="open-external-git" aria-label="在外部 Git 软件中打开" title="外部 Git 软件">${icon("external")}</button>` : ""}<button class="icon-button" data-action="rename-resource" aria-label="重命名资源" title="重命名">${icon("edit")}</button><button class="icon-button" data-action="delete-resource" aria-label="删除资源" title="删除">${icon("trash")}</button>`;
    let body;
    if (isExternalDoc) {
      body = `<div class="empty-state attachment-open"><span class="file-icon attachment">${icon("external")}</span><strong>通过外部应用打开</strong><span>这是虚构附件；点击仅显示演示提示，不会读取或打开本机文件。</span><button class="button compact" data-action="external-file">${icon("external")}打开方式</button></div>`;
    } else if (isImage) {
      body = `<div class="attachment-preview"><svg viewBox="0 0 560 300" role="img" aria-label="巷口春日的虚构示例图片"><defs><linearGradient id="sky" x2="0" y2="1"><stop stop-color="#e8edf4"/><stop offset="1" stop-color="#f7efe2"/></linearGradient></defs><rect width="560" height="300" rx="12" fill="url(#sky)"/><circle cx="430" cy="70" r="34" fill="#f3d8a7"/><path d="M0 205 95 137l72 54 91-88 84 95 80-62 138 81v83H0z" fill="#c9d5c2"/><path d="M0 240 130 180l89 66 112-54 103 41 126-42v109H0z" fill="#a6b99e"/><path d="M219 300V202l63-43 62 43v98z" fill="#ded0bd"/><path d="M235 300v-80h93v80z" fill="#f6f0e8"/><path d="M248 237h27v32h-27zM288 237h27v32h-27z" fill="#aebfc0"/><text x="25" y="38" fill="#6c7180" font-size="14" font-family="sans-serif">示例图片 · 巷口的春天</text></svg><p>图片预览 · 虚构示例</p></div>`;
    } else if (state.resourceMode === "edit" && resource.type === "doc") {
      body = `<textarea class="markdown-editor" aria-label="示例文档编辑区" data-draft>${escapeHtml(value)}</textarea><div class="demo-edit-note">编辑仅保存在当前页面内存中；不会写入文件。<button class="quiet-link" data-action="save-demo">保存演示内容</button></div>`;
    } else if (state.resourceMode === "edit" && (resource.type === "repo" || resource.type === "script")) {
      body = `<textarea class="markdown-editor" aria-label="示例代码编辑区" data-draft>${escapeHtml(value)}</textarea><div class="demo-edit-note">轻量编辑展示；当前页面不会写入文件。<button class="quiet-link" data-action="save-demo">保存演示内容</button></div>`;
    } else if (resource.type === "repo" || resource.type === "script") {
      body = `<pre class="markdown-preview code-preview">${escapeHtml(value)}</pre>${resource.type === "repo" ? `<div class="demo-edit-note">仓库文件可轻量编辑；Git 操作请使用外部 Git 软件。</div>` : ""}`;
    } else {
      body = `<article class="markdown-preview">${renderMarkdown(value)}</article>`;
    }
    const isStaticAttachment = resource.type === "attachment";
    const tabs = isExternalDoc || isStaticAttachment ? "" : `<div class="preview-tabs"><button class="preview-tab ${state.resourceMode === "preview" ? "active" : ""}" data-resource-mode="preview">${resource.type === "doc" ? "预览" : "浏览"}</button><button class="preview-tab ${state.resourceMode === "edit" ? "active" : ""}" data-resource-mode="edit">轻量编辑</button></div>`;
    const typeClass = resource.type === "repo" ? "repo" : resource.type === "script" ? "script" : resource.type === "attachment" ? "attachment" : "doc";
    const glyph = resource.type === "repo" ? "code" : resource.type === "script" ? "terminal" : resource.type === "attachment" ? "image" : "file";
    const projectAssociation = resource.type === "repo" ? ` · 关联项目：${escapeHtml(resource.project)}` : "";
    return `<section class="panel resource-detail"><div class="resource-detail-head"><span class="file-icon ${typeClass}">${icon(glyph)}</span><div class="resource-detail-title"><h2>${escapeHtml(resource.title)}</h2><p>${escapeHtml(resource.path)} · ${escapeHtml(resource.detail)}${projectAssociation}</p></div><div class="resource-actions">${actions}</div></div>${tabs}${body}</section>`;
  }

  function renderResourceBrowser(category) {
    const list = filteredResources(category);
    if (!list.some((resource) => resource.id === state.selectedResource)) {
      const fallback = list[0];
      if (fallback) state.selectedResource = fallback.id;
    }
    const labels = { repo: "仓库", doc: "文档", attachment: "附件", script: "脚本" };
    return `<div class="resources-layout"><section class="panel resource-browser"><div class="resource-toolbar"><label class="search-box">${icon("search")}<input id="resource-search" type="search" value="${escapeHtml(state.filter)}" placeholder="搜索${labels[category]}" aria-label="搜索${labels[category]}" autocomplete="off"></label><button class="icon-button" data-action="resource-menu" aria-label="${labels[category]}选项" title="${labels[category]}选项">${icon("dots")}</button></div><div class="resource-list">${list.length ? list.map(resourceListItem).join("") : `<div class="resource-empty">没有匹配的示例${labels[category]}</div>`}</div></section>${renderResourceDetail(category)}</div>`;
  }

  function noteSummary(content) {
    const line = content.split("\n").map((part) => part.replace(/^#{1,6}\s+|^[-*>]\s+/, "").trim()).find(Boolean) || "暂无内容";
    return `${line.slice(0, 58)}${line.length > 58 ? "…" : ""}`;
  }

  function noteUpdated(detail) {
    const parts = detail.split("·");
    return parts.length > 1 ? parts[parts.length - 1].trim() : detail;
  }

  function renderNotesWorkspace() {
    const notes = filteredResources("doc");
    if (notes.length && !notes.some((note) => note.id === state.selectedResource)) {
      state.selectedResource = notes[0].id;
      state.resourceMode = "preview";
      state.draft = null;
    }
    const selected = notes.find((note) => note.id === state.selectedResource);
    const rows = notes.map((note) => `<button class="note-row ${note.id === state.selectedResource ? "selected" : ""}" data-resource="${escapeHtml(note.id)}" ${note.id === state.selectedResource ? 'aria-current="true"' : ""}><strong>${escapeHtml(note.title)}</strong><span>${escapeHtml(noteSummary(note.content))}</span><small>${escapeHtml(noteUpdated(note.detail))}</small></button>`).join("");
    const detail = selected ? renderResourceDetail("doc") : `<section class="notes-empty"><div class="empty-state">${state.filter ? "没有匹配的笔记" : "选择一篇笔记查看内容"}</div></section>`;
    return `<section class="notes-workspace" aria-label="文档笔记"><aside class="notes-list-pane"><div class="notes-list-head"><div><h2>笔记</h2><span>${notes.length} 篇</span></div><button class="icon-button" data-action="new-resource" aria-label="新建笔记" title="新建笔记">${icon("plus")}</button></div><label class="search-box notes-search">${icon("search")}<input id="resource-search" type="search" value="${escapeHtml(state.filter)}" placeholder="搜索笔记" aria-label="搜索笔记" autocomplete="off"></label><div class="notes-list">${rows || `<div class="resource-empty">没有匹配的笔记</div>`}</div></aside><div class="notes-detail-pane">${detail}</div></section>`;
  }

  function renderResources() {
    if (state.category === "doc") return renderNotesWorkspace();
    const pageConfig = {
      repo: { eyebrow: "Git 管理", title: "本地仓库", description: "查看仓库与项目关联；Git 操作交给外部 Git 软件。", add: "仓库", extra: `<button class="button" data-action="open-external-git">${icon("external")}外部 Git 软件</button>` },
      attachment: { eyebrow: "附件管理", title: "项目附件", description: "浏览图片、PDF 与 Word 等虚构附件示例。", add: "附件", extra: "" },
    }[state.category] || { eyebrow: "资源管理", title: "资源", description: "虚构示例内容。" };
    const actions = `${pageConfig.extra || ""}<button class="button primary" data-action="new-resource">${icon("plus")}添加${pageConfig.add || "资源"}</button>`;
    return `${pageHead(pageConfig.eyebrow, pageConfig.title, pageConfig.description, actions)}${renderResourceBrowser(state.category)}`;
  }

  function renderShortcuts() {
    const tabs = `<div class="shortcut-tabs" role="tablist" aria-label="快捷按钮与脚本"><button class="shortcut-tab ${state.shortcutTab === "buttons" ? "active" : ""}" role="tab" aria-selected="${state.shortcutTab === "buttons"}" data-shortcut-tab="buttons">快捷按钮</button><button class="shortcut-tab ${state.shortcutTab === "scripts" ? "active" : ""}" role="tab" aria-selected="${state.shortcutTab === "scripts"}" data-shortcut-tab="scripts">脚本</button></div>`;
    if (state.shortcutTab === "scripts") {
      state.category = "script";
      return `${pageHead("快捷按钮 / 脚本", "脚本", "查看虚构脚本示例，并在页面内进行轻量编辑。", `<button class="button primary" data-action="new-resource">${icon("plus")}添加脚本</button>`)}${tabs}${renderResourceBrowser("script")}`;
    }
    const cards = shortcuts.map((shortcut) => `<button class="shortcut-card" data-shortcut="${shortcut.id}"><span class="shortcut-icon" style="background:${shortcut.tint}">${icon(shortcut.icon)}</span><strong>${shortcut.title}</strong><small>${shortcut.note}</small></button>`).join("");
    return `${pageHead("快捷按钮", "常用入口，伸手可及", "打开项目、应用和终端，或运行自定义脚本。", `<button class="button primary" data-action="new-shortcut">${icon("plus")}添加快捷按钮</button>`)}${tabs}
      <div class="section-row" style="margin-top:0"><h2 class="section-heading">我的快捷按钮</h2><span class="section-hint">5 个示例</span></div><section class="shortcut-grid">${cards}<button class="shortcut-card add-card" data-action="new-shortcut"><span class="shortcut-icon">${icon("plus")}</span><strong>添加快捷按钮</strong><small>配置项目、应用或命令</small></button></section>`;
  }

  function renderSettings() {
    const row = (title, description, value, action, button = "修改") => `<div class="settings-row"><div class="settings-copy"><strong>${title}</strong><span>${description}</span></div><span class="settings-value">${value}</span><button class="button compact" data-action="setting-option" data-setting="${action}">${button}</button></div>`;
    return `${pageHead("工作台偏好", "设置", "浏览设置示例；这些选项不会改变系统或本机配置。")}
      <div class="settings-layout">
        <section class="panel settings-panel"><div class="settings-section-head"><span class="settings-symbol">${icon("settings")}</span><div><h2>通用</h2><p>工作台的基本行为</p></div></div>${row("启动时显示", "打开工作台后首先进入的页面", "首页", "startup-page")}${row("默认打开方式", "不同类型文件的预览与外部打开偏好", "按文件类型选择", "open-preferences")}</section>
        <section class="panel settings-panel"><div class="settings-section-head"><span class="settings-symbol">${icon("folder")}</span><div><h2>工作台目录</h2><p>项目和资源所在的位置</p></div></div>${row("当前目录", "此处为虚构路径，不会访问本机", "示例目录 / AI 工作台", "workspace-directory", "选择目录")}</section>
        <section class="panel settings-panel"><div class="settings-section-head"><span class="settings-symbol">${icon("external")}</span><div><h2>默认打开方式</h2><p>本地应用与文件类型关联</p></div></div>${row("PDF 与 Word", "通过系统中的外部应用打开", "系统默认应用", "external-apps")}${row("代码文件", "可在工作台轻量查看和编辑", "工作台 / 外部编辑器", "code-editor")}</section>
        <section class="panel settings-panel"><div class="settings-section-head"><span class="settings-symbol">${icon("spark")}</span><div><h2>外观</h2><p>工作台界面偏好</p></div></div><div class="settings-row"><div class="settings-copy"><strong>界面主题</strong><span>浅色、深色或跟随系统</span></div><div class="setting-choice"><button class="choice-selected" data-action="setting-option" data-setting="theme-light">浅色</button><button data-action="setting-option" data-setting="theme-dark">深色</button><button data-action="setting-option" data-setting="theme-auto">自动</button></div></div></section>
      </div>`;
  }

  function escapeHtml(value) {
    return String(value).replaceAll("&", "&amp;").replaceAll("<", "&lt;").replaceAll(">", "&gt;").replaceAll('"', "&quot;").replaceAll("'", "&#39;");
  }

  function showToast(message) {
    const toast = $("#toast");
    toast.textContent = message;
    toast.classList.add("visible");
    window.clearTimeout(state.toastTimer);
    state.toastTimer = window.setTimeout(() => toast.classList.remove("visible"), 2800);
  }

  function openModal(kind) {
    state.modal = kind;
    state.modalQuery = "";
    renderModal();
    const input = $("#modal-input");
    if (input) input.focus();
  }

  function closeModal() {
    state.modal = null;
    $("#modal-root").innerHTML = "";
  }

  function modalContent() {
    const resource = currentResource();
    if (state.modal === "search") return { title: "搜索示例工作区", body: `搜索项目、资源与会话。<label class="search-box" style="margin-top:10px">${icon("search")}<input id="modal-input" type="search" placeholder="输入项目、文档或会话名称" value="${escapeHtml(state.modalQuery)}" autocomplete="off"></label><div id="search-results" class="modal-results">${searchResults()}</div>`, confirm: "完成" };
    if (state.modal === "project-directory") return { title: "项目目录", body: `全部 ${projects.length} 个示例项目。选择一个项目打开详情。<div class="project-directory-list">${projects.map((project) => `<button class="${project.id === state.projectId ? "current" : ""}" data-action="directory-project" data-project="${project.id}" ${project.id === state.projectId ? 'aria-current="true"' : ""}>${projectMark(project)}<span><strong>${project.name}</strong><small>${project.description}</small></span>${icon("arrow")}</button>`).join("")}</div>`, confirm: "关闭" };
    if (state.modal === "help") return { title: "这是一个界面预览", body: "所有项目、资源和会话均为虚构示例。按钮用于展示预期交互，不会访问本机文件、创建软链接、保存真实文件或启动外部 Agent。", confirm: "明白了" };
    if (state.modal === "session") {
      const session = sessions.find((item) => item.id === state.modalSession);
      return { title: "恢复项目会话 · 演示", body: `${session ? `${session.agent} · ${session.project} · ${session.title}` : "示例会话"}<br><br>会话内容将在原生 Agent 中继续。本原型不会读取、恢复或启动真实会话。`, confirm: "演示恢复", action: "resume-session-confirm" };
    }
    if (state.modal === "new-resource") return { title: "添加资源 · 界面演示", body: "实际工作台会支持在项目中管理仓库、文档和脚本。此原型不创建文件或 Git 仓库，也不会改动目录。", confirm: "知道了" };
    if (state.modal === "rename-resource") return { title: "重命名资源 · 界面演示", body: `示例资源“${resource.title}”不会被真实重命名。名称、目录和关联均保持不变。`, confirm: "知道了" };
    if (state.modal === "delete-resource") return { title: "删除资源 · 界面演示", body: `示例资源“${resource.title}”不会被删除。实际删除行为和关联影响不在此原型中演示。`, confirm: "知道了" };
    const titles = { "new-project": "新建项目 · 界面演示", "new-shortcut": "添加快捷按钮 · 界面演示", "project-settings": "项目选项 · 界面演示", "resource-menu": "资源选项 · 界面演示", profile: "个人空间 · 示例数据" };
    const descriptions = { "new-project": "这里展示项目创建入口。本原型不会在本机新建项目目录。", "new-shortcut": "快捷按钮可打开项目、应用、终端或运行自定义脚本/命令。此处只展示演示反馈，不会保存配置。", "project-settings": "项目名称、说明及资源关联均为虚构示例。", "resource-menu": "资源筛选和选择可交互；管理操作仅作界面预览。", profile: "当前页面使用虚构的个人空间信息。" };
    return { title: titles[state.modal] || "操作预览", body: descriptions[state.modal] || "此处为界面演示，不会执行文件系统操作。", confirm: "知道了" };
  }

  function searchResults() {
    const query = state.modalQuery.trim().toLowerCase();
    if (!query) return `<div class="empty-state">试试搜索“栖居计划”或“项目说明”</div>`;
    const hits = [
      ...projects.filter((item) => item.name.toLowerCase().includes(query)).map((item) => ({ kind: "项目", title: item.name, action: "project", id: item.id })),
      ...resources.filter((item) => `${item.title} ${item.path}`.toLowerCase().includes(query)).map((item) => ({ kind: item.type === "repo" ? "Git 仓库" : item.type === "doc" ? "文档" : item.type === "attachment" ? "附件" : "脚本", title: item.title, action: "resource", id: item.id })),
      ...sessions.filter((item) => `${item.title} ${item.agent}`.toLowerCase().includes(query)).map((item) => ({ kind: "会话", title: item.title, action: "session", id: item.id })),
    ].slice(0, 6);
    if (!hits.length) return `<div class="empty-state">没有找到匹配的示例内容</div>`;
    return hits.map((hit) => `<button class="search-hit" data-search-kind="${hit.action}" data-search-id="${hit.id}"><span>${hit.title}</span><small>${hit.kind}</small></button>`).join("");
  }

  function renderModal() {
    const root = $("#modal-root");
    if (!state.modal) { root.innerHTML = ""; return; }
    const content = modalContent();
    root.innerHTML = `<div class="modal-backdrop" data-action="backdrop-close"><section class="modal" role="dialog" aria-modal="true" aria-labelledby="modal-title"><div class="modal-icon">${icon(state.modal === "search" ? "search" : state.modal === "session" ? "message" : "spark")}</div><h2 id="modal-title">${content.title}</h2><div class="modal-copy">${content.body}</div><div class="modal-actions"><button class="button primary" data-action="modal-confirm" ${content.action ? `data-confirm-action="${content.action}"` : ""}>${content.confirm}</button></div></section></div>`;
  }

  function switchProject(projectId) {
    if (!projects.some((project) => project.id === projectId)) return;
    state.projectId = projectId;
    state.page = "project";
    state.workspaceTab = "overview";
    render();
  }

  function goToSearchHit(kind, id) {
    closeModal();
    if (kind === "project") switchProject(id);
    if (kind === "resource") {
      const item = resources.find((resource) => resource.id === id);
      if (item) navigateToResource(item);
    }
    if (kind === "session") showSession(id);
  }

  function showSession(sessionId) {
    const session = sessions.find((item) => item.id === sessionId);
    if (!session) return;
    state.modalSession = sessionId;
    state.modal = "session";
    renderModal();
  }

  function handleAction(action, target, event) {
    if (action === "open-project") {
      if (target.dataset.project) switchProject(target.dataset.project);
      else { state.page = "project"; render(); }
      if (target.dataset.tab === "sessions") { state.workspaceTab = "sessions"; render(); }
    }
    else if (action === "search") openModal("search");
    else if (["help", "new-project", "new-shortcut", "new-resource", "rename-resource", "delete-resource", "project-settings", "resource-menu", "show-project-directory"].includes(action)) openModal(action === "show-project-directory" ? "project-directory" : action);
    else if (action === "open-terminal") showToast("终端启动仅作演示；不会打开外部程序。");
    else if (action === "external-file") showToast("外部应用打开仅作演示；没有访问本机文件。");
    else if (action === "open-external-git") showToast("外部 Git 软件入口仅作演示；不会打开应用或执行 Git 操作。");
    else if (action === "setting-option") showToast("此设置仅作界面示例；系统设置和工作台配置没有改变。");
    else if (action === "save-demo") {
      const resource = resources.find((item) => item.id === state.selectedResource);
      if (resource && state.draft !== null) resource.content = state.draft;
      state.draft = null;
      state.resourceMode = "preview";
      render();
      showToast("演示内容已保存到当前页面内存；刷新后会恢复虚构示例。");
    }
    else if (action === "open-resource") {
      const resource = resources.find((item) => item.id === target.dataset.resource);
      if (resource) navigateToResource(resource);
    }
    else if (action === "resume-session") { showSession(target.dataset.session); }
    else if (action === "modal-confirm") {
      const confirmAction = target.dataset.confirmAction;
      closeModal();
      if (confirmAction === "resume-session-confirm") showToast("会话恢复为演示流程；不会启动 Agent。");
    }
    else if (action === "backdrop-close" && event.target === target && target.classList.contains("modal-backdrop")) closeModal();
  }

  document.addEventListener("click", (event) => {
    const target = event.target.closest("button, [data-action]");
    if (!target) return;
    if (target.dataset.page) {
      state.page = target.dataset.page;
      if (target.dataset.category) { state.category = target.dataset.category; state.filter = ""; state.resourceMode = "preview"; state.draft = null; }
      if (state.page === "shortcuts") state.shortcutTab = "buttons";
      render();
      return;
    }
    if (target.dataset.action === "open-resource") { handleAction(target.dataset.action, target, event); return; }
    if (target.dataset.action === "directory-project") { const projectId = target.dataset.project; closeModal(); switchProject(projectId); return; }
    if (target.dataset.project && target.dataset.action === "open-project") {
      switchProject(target.dataset.project);
      if (target.dataset.tab === "sessions") { state.workspaceTab = "sessions"; render(); }
      return;
    }
    if (target.dataset.project) { switchProject(target.dataset.project); return; }
    if (target.dataset.shortcutTab) { state.shortcutTab = target.dataset.shortcutTab; state.filter = ""; state.resourceMode = "preview"; state.draft = null; render(); return; }
    if (target.dataset.resourceMode) { state.resourceMode = target.dataset.resourceMode; if (state.resourceMode === "edit" && state.draft === null) state.draft = currentResource().content; render(); return; }
    if (target.dataset.workspaceTab) { state.workspaceTab = target.dataset.workspaceTab; render(); return; }
    if (target.dataset.resource) { state.selectedResource = target.dataset.resource; state.category = currentResource().type; state.resourceMode = "preview"; state.draft = null; render(); return; }
    if (target.dataset.session) { showSession(target.dataset.session); return; }
    if (target.dataset.agent) { showToast(`启动 ${target.dataset.agent} 仅作演示；不会打开外部 Agent。`); return; }
    if (target.dataset.shortcut) { const item = shortcuts.find((shortcut) => shortcut.id === target.dataset.shortcut); showToast(`${item ? item.title : "快捷按钮"}：此入口仅作演示，不会打开或运行任何内容。`); return; }
    if (target.dataset.searchKind) { goToSearchHit(target.dataset.searchKind, target.dataset.searchId); return; }
    if (target.dataset.action) handleAction(target.dataset.action, target, event);
  });

  document.addEventListener("input", (event) => {
    if (event.target.id === "resource-search") { state.filter = event.target.value; render(); const input = $("#resource-search"); if (input) { input.focus(); input.setSelectionRange(input.value.length, input.value.length); } }
    if (event.target.matches("[data-draft]")) state.draft = event.target.value;
    if (event.target.id === "modal-input") { state.modalQuery = event.target.value; const results = $("#search-results"); if (results) results.innerHTML = searchResults(); const input = $("#modal-input"); if (input) { input.focus(); input.setSelectionRange(input.value.length, input.value.length); } }
  });

  document.addEventListener("keydown", (event) => {
    if (event.key === "Escape" && state.modal) closeModal();
    if ((event.metaKey || event.ctrlKey) && event.key.toLowerCase() === "k") { event.preventDefault(); openModal("search"); }
  });

  function boot() {
    document.querySelectorAll("[data-icon]").forEach((element) => { element.innerHTML = icon(element.dataset.icon); });
    render();
  }

  boot();
})();
