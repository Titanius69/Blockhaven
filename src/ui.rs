use crate::model::*;
use fastchapp_desktop::escape as e;
pub fn icon(name: &str) -> String {
    let paths=match name{
 "home"=>"<path d='m3 10 9-7 9 7v10a1 1 0 0 1-1 1h-5v-7H9v7H4a1 1 0 0 1-1-1Z'/>",
 "profiles"=>"<rect x='3' y='3' width='7' height='7' rx='1.5'/><rect x='14' y='3' width='7' height='7' rx='1.5'/><rect x='3' y='14' width='7' height='7' rx='1.5'/><rect x='14' y='14' width='7' height='7' rx='1.5'/>",
 "mods"=>"<path d='M8 3h4v4a2 2 0 1 0 4 0V3h4v6h-3a2 2 0 1 0 0 4h3v7h-6v-3a2 2 0 1 0-4 0v3H3v-7h3a2 2 0 1 0 0-4H3V3Z'/>",
 "search"=>"<circle cx='10.5' cy='10.5' r='6.5'/><path d='m16 16 5 5'/>",
 "accounts"=>"<circle cx='12' cy='8' r='4'/><path d='M4 21v-2a8 8 0 0 1 16 0v2'/>",
 "settings"=>"<path d='m9 3 1-1h4l1 1v3l3 2 3-1 2 4-2 2v3l2 2-2 4-3-1-3 2H10l-3-2-3 1-2-4 2-2v-3l-2-2 2-4 3 1 3-2V3Z' transform='translate(0 -1) scale(.95)'/><circle cx='12' cy='12' r='3'/>",
 "logs"=>"<rect x='3' y='4' width='18' height='16' rx='3'/><path d='m7 9 3 3-3 3m6 0h4'/>",
 "plus"=>"<path d='M12 5v14M5 12h14'/>","arrow"=>"<path d='M5 12h14m-5-5 5 5-5 5'/>",
 "play"=>"<path d='m8 5 12 7-12 7Z'/>","stop"=>"<rect x='6' y='6' width='12' height='12' rx='1'/>",
 "download"=>"<path d='M12 3v12m-5-5 5 5 5-5M4 16v4h16v-4'/>",
 "refresh"=>"<path d='M20 8a8 8 0 0 0-14-3L3 8m0-5v5h5m-4 8a8 8 0 0 0 14 3l3-3m0 5v-5h-5'/>",
 "folder"=>"<path d='M3 7V5h6l2 2h10v13H3Zm0 3h18'/>","check"=>"<path d='m5 12 4 4L19 6'/>",
 "cube"=>"<path d='m12 3 9 5v9l-9 5-9-5V8Zm-9 5 9 5 9-5m-9 5v9'/>",
 "chevron"=>"<path d='m8 10 4 4 4-4'/>","close"=>"<path d='m6 6 12 12M6 18 18 6'/>",
 "minimize"=>"<path d='M5 12h14'/>","maximize"=>"<rect x='5' y='5' width='14' height='14' rx='1'/>",
 "back"=>"<path d='M19 12H5m5-5-5 5 5 5'/>","external"=>"<path d='M14 3h7v7m0-7L10 14m-1-9H3v16h16v-6'/>",
 "shield"=>"<path d='m12 3 8 3v6c0 5-8 9-8 9s-8-4-8-9V6Z'/><path d='m8 12 3 3 5-5'/>",
 _=>"<circle cx='12' cy='12' r='8'/><path d='M12 7v5l3 2'/>",};
    format!("<svg class=\"icon\" viewBox=\"0 0 24 24\" aria-hidden=\"true\" fill=\"none\" stroke=\"currentColor\" stroke-width=\"1.65\" stroke-linecap=\"round\" stroke-linejoin=\"round\">{paths}</svg>")
}
fn button(action: &str, value: &str, label: &str, class: &str) -> String {
    format!("<button type=\"button\" class=\"{class}\" data-action=\"{action}\" data-value=\"{}\">{label}</button>",e(value))
}
fn tool(action: &str, value: &str, label: &str, glyph: &str) -> String {
    format!("<button type=\"button\" class=\"icon-button\" title=\"{label}\" aria-label=\"{label}\" data-action=\"{action}\" data-value=\"{}\">{}</button>",e(value),icon(glyph))
}
fn field(label: &str, name: &str, value: &str, kind: &str) -> String {
    format!(
        "<label>{label}<input name=\"{name}\" type=\"{kind}\" value=\"{}\" required></label>",
        e(value)
    )
}
fn select(
    options: impl IntoIterator<Item = (String, String)>,
    name: &str,
    current: &str,
) -> String {
    format!(
        "<select name=\"{name}\">{}</select>",
        options
            .into_iter()
            .map(|(v, t)| format!(
                "<option value=\"{}\" {}>{}</option>",
                e(&v),
                if v == current { "selected" } else { "" },
                e(&t)
            ))
            .collect::<String>()
    )
}
fn badge(text: &str) -> String {
    format!("<span class=\"badge\">{}</span>", e(text))
}
fn submit(text: &str) -> String {
    format!("<button class=\"primary\" type=\"submit\">{text}</button>")
}
fn empty(glyph: &str, title: &str, note: &str, action: String) -> String {
    format!("<div class=\"empty\"><div class=\"empty-icon\">{}</div><h3>{title}</h3><p>{note}</p>{action}</div>",icon(glyph))
}
pub fn icon_class(project: &str) -> String {
    use sha1::Digest;
    format!(
        "mod-art-{}",
        hex::encode(sha1::Sha1::digest(project.as_bytes()))
    )
}
pub fn mod_art(s: &State, project: &str, title: &str) -> String {
    let content = if let Some(src) = s.icons.get(project) {
        format!(
            "<img src=\"{}\" alt=\"{} icon\" width=\"42\" height=\"42\">",
            e(src),
            e(title)
        )
    } else {
        format!(
            "<span class=\"art-placeholder\" aria-label=\"{} icon unavailable\">{}</span>",
            e(title),
            e(&title.chars().take(1).collect::<String>().to_uppercase())
        )
    };
    format!(
        "<div class=\"mod-art {}\">{content}</div>",
        icon_class(project)
    )
}
pub fn document(s: &State) -> String {
    include_str!("../resources/index.html").replace("__APP__", &shell(s))
}
pub fn shell(s: &State) -> String {
    let nav = [
        ("home", "home", "Home"),
        ("profiles", "profiles", "Instances"),
        ("modrinth", "search", "Discover mods"),
        ("installed", "mods", "Installed mods"),
        ("accounts", "accounts", "Accounts"),
        ("settings", "settings", "Settings"),
        ("logs", "logs", "Console"),
    ]
    .iter()
    .map(|(id, glyph, label)| {
        let count = if *id == "profiles" {
            s.config.profiles.len()
        } else if *id == "installed" {
            s.config.profile().map(|p| s.mod_counts(p).0).unwrap_or(0)
        } else {
            0
        };
        let suffix = if count > 0 {
            format!("<span class=\"nav-count\">{count}</span>")
        } else {
            String::new()
        };
        button(
            "navigate",
            id,
            &format!("{}<span>{label}</span>{suffix}", icon(glyph)),
            if s.page == *id { "nav active" } else { "nav" },
        )
    })
    .collect::<String>();
    let (account, mode) = s
        .session
        .as_ref()
        .map(|a| {
            (
                e(&a.name),
                if a.offline {
                    "Offline account"
                } else {
                    "Microsoft account"
                },
            )
        })
        .unwrap_or_else(|| ("Select an account".into(), "Not signed in"));
    let active=s.config.profile().map(|p|format!("<span class=\"live-dot\"></span><span>{}</span><span class=\"context-version\">{}</span>",e(&p.name),e(&p.version))).unwrap_or_else(|_|"<span>No instance selected</span>".into());
    let progress = s.progress.detail.lock().unwrap().2.clone();
    let pct = s.progress.percent();
    format!(
        r#"<div class="app-shell"><div class="titlebar" data-window-drag><div class="window-brand"><img src="/icons/mark.svg" alt="" width="15" height="15"><span>Blockhaven</span><span class="titlebar-version">0.3.2</span></div><div class="titlebar-caption">Minecraft, thoughtfully managed.</div><div class="window-controls">{}{}{}</div></div><div class="app-content"><aside><div class="brand"><img src="/icons/blockhaven.svg" alt="Blockhaven" width="36" height="36"><div>Blockhaven<small>JAVA EDITION LAUNCHER</small></div></div><div class="nav-label">WORKSPACE</div><nav>{nav}</nav><div class="sidebar-bottom"><div class="account-card">{}<div class="account-avatar">{}</div><div class="account-copy"><strong>{account}</strong><small>{mode}</small></div><span class="account-chevron">{}</span></div><div class="sidebar-meta"><span class="live-dot"></span> All your worlds, one place.</div></div></aside><section class="workspace"><div class="contextbar">{}{}</div><main id="view">{}</main><footer><div class="status-dot {}"></div><span id="status">{}</span><span id="progress-label">{}</span><progress id="progress" max="100" value="{}" {}></progress><button type="button" class="cancel-button" data-action="cancel" {}>Cancel</button><span class="footer-meta">BLOCKHAVEN 0.3.2</span></footer></section></div><div id="error" role="alert">{}</div><div class="resize-edge n" data-window-resize="north"></div><div class="resize-edge s" data-window-resize="south"></div><div class="resize-edge e" data-window-resize="east"></div><div class="resize-edge w" data-window-resize="west"></div><div class="resize-edge ne" data-window-resize="north_east"></div><div class="resize-edge nw" data-window-resize="north_west"></div><div class="resize-edge se" data-window-resize="south_east"></div><div class="resize-edge sw" data-window-resize="south_west"></div></div>"#,
        tool("window_minimize", "", "Minimize", "minimize"),
        tool(
            "window_toggle_maximize",
            "",
            "Maximize or restore",
            "maximize"
        ),
        tool("window_close", "", "Close", "close"),
        button(
            "navigate",
            "accounts",
            r#"<span class="sr-only">Manage accounts</span>"#,
            "account-hit"
        ),
        icon("accounts"),
        icon("chevron"),
        button(
            "navigate",
            "profiles",
            &format!("{active}{}", icon("chevron")),
            "context-instance"
        ),
        button(
            "refresh",
            "",
            &format!("{} Refresh catalog", icon("refresh")),
            "context-action"
        ),
        page(s),
        if s.busy { "working" } else { "" },
        e(&s.status),
        if s.busy { e(&progress) } else { String::new() },
        if s.busy { pct } else { 0 },
        if s.busy { "" } else { "hidden" },
        if s.busy { "" } else { "hidden" },
        if s.error.is_empty() {
            String::new()
        } else {
            format!(
                "<span>{}</span>{}",
                e(&s.error),
                tool("dismiss_error", "", "Dismiss message", "close")
            )
        }
    )
}
fn heading(title: &str, sub: &str, actions: String) -> String {
    format!("<div class=\"page-heading\"><div><h1>{title}</h1><p>{sub}</p></div><div class=\"actions\">{actions}</div></div>")
}
pub fn page(s: &State) -> String {
    match s.page.as_str(){"profiles"=>profiles(s),"modrinth"=>modrinth(s),"installed"=>installed(s),"accounts"=>accounts(s),"settings"=>settings(s),"logs"=>format!("{}<section class=\"console-panel\"><div class=\"console-header\"><span class=\"live-dot\"></span> Application &amp; game output <span>Last 250 lines</span></div><pre class=\"logs\" id=\"log-lines\">{}</pre></section>",heading("Console","A clear view of everything happening behind the scenes.",tool("clear_logs","","Clear console","close")),e(&s.logs.join("\n"))),_=>home(s)}
}
fn home(s: &State) -> String {
    let mut out = heading(
        "Welcome home.",
        "Your next adventure is a click away.",
        s.config
            .profile()
            .map(|p| {
                format!(
                    "{}{}",
                    tool("open_profile", &p.id, "Open instance folder", "folder"),
                    tool("open_version", &p.id, "Open version folder", "cube")
                )
            })
            .unwrap_or_default(),
    );
    let (name, detail, action) = if let Ok(p) = s.config.profile() {
        let installed = s.installed(p);
        let (label, glyph, act) = if s.running {
            ("Stop Minecraft", "stop", "stop")
        } else if installed {
            ("Launch Minecraft", "play", "play")
        } else {
            ("Install Minecraft", "download", "install")
        };
        (
            e(&p.name),
            format!(
                "{} {} <span>·</span> {} memory",
                badge(&format!("Minecraft {}", p.version)),
                badge(if p.loader == "fabric" {
                    "Fabric"
                } else {
                    "Vanilla"
                }),
                ram(p.memory_mb)
            ),
            button(
                act,
                "",
                &format!("{} {label}", icon(glyph)),
                "primary launch",
            ),
        )
    } else {
        (
            "A little space.<br>A whole new world.".into(),
            "Create an instance. Choose your version.<br>Make it yours.".into(),
            button(
                "navigate",
                "profiles",
                &format!("{} Create your first instance", icon("plus")),
                "primary launch",
            ),
        )
    };
    out.push_str(&format!("<section class=\"hero\"><div class=\"hero-content\"><span class=\"eyebrow\">YOUR WORLD STARTS HERE</span><h2>{name}</h2><div class=\"hero-detail\">{detail}</div><div class=\"actions\">{action}{}</div></div><div class=\"hero-art\"><img src=\"/icons/landscape.svg\" alt=\"\" width=\"620\" height=\"360\"></div><span class=\"hero-coordinate\">OVERWORLD / JAVA EDITION</span></section>",button("navigate","profiles","Manage instances","hero-secondary")));
    out.push_str("<div class=\"section-title\"><h2>Your workspace</h2><span>Everything in its own place</span></div><div class=\"quick-grid\">");
    for (glyph, title, count, note, page) in [
        (
            "profiles",
            "Instances",
            s.config.profiles.len(),
            "Separate worlds & versions",
            "profiles",
        ),
        (
            "mods",
            "Installed mods",
            s.config.profile().map(|p| s.mod_counts(p).0).unwrap_or(0),
            "Built for your selected instance",
            "installed",
        ),
        (
            "accounts",
            "Offline accounts",
            s.config.offline_accounts.len(),
            "Switch players, keep exploring",
            "accounts",
        ),
    ] {
        out.push_str(&button("navigate",page,&format!("<div class=\"quick-top\"><span class=\"quick-icon\">{}</span><strong>{count}</strong></div><h3>{title}</h3><p>{note}</p><span class=\"quick-arrow\">{}</span>",icon(glyph),icon("arrow")),"quick-card"));
    }
    out.push_str("</div>");
    out.push_str(&format!("<section class=\"release-strip\"><span class=\"release-icon\">{}</span><div><h3>Minecraft {} <span>Latest release</span></h3><p>{}</p></div>{}</section>",icon("cube"),e(&s.manifest.latest.release),if s.online{"Official catalog synced for this session."}else{"Bundled official catalog. Refresh to check for new releases."},tool("refresh","","Refresh official catalog","refresh")));
    out
}
fn ram(m: u32) -> String {
    if m.is_multiple_of(1024) {
        format!("{} GiB", m / 1024)
    } else {
        format!("{} MiB", m)
    }
}
fn profiles(s: &State) -> String {
    let mut out = heading(
        "Instances",
        "Different versions. Separate worlds. Zero clutter.",
        String::new(),
    );
    out.push_str("<div class=\"split-layout\"><section class=\"instance-list\">");
    for p in &s.config.profiles {
        let active = s.config.selected.as_deref() == Some(&p.id);
        out.push_str(&format!("<article class=\"instance-card {}\"><div class=\"instance-art {}\">{}</div><div class=\"grow\"><h3>{}</h3><p>Minecraft {} <span>·</span> {} <span>·</span> {}</p><div class=\"instance-status\"><span class=\"{}\"></span>{}</div></div><div class=\"instance-actions\">{}{}{}</div></article>",if active{"selected"}else{""},p.loader,icon("cube"),e(&p.name),e(&p.version),if p.loader=="fabric"{"Fabric"}else{"Vanilla"},ram(p.memory_mb),if s.installed(p){"live-dot"}else{"muted-dot"},if s.installed(p){"Ready to launch"}else{"Not installed"},button("select_profile",&p.id,&if active{format!("{} Selected",icon("check"))}else{"Select".into()},if active{"selected-button small"}else{"secondary small"}),tool("open_profile",&p.id,"Open instance folder","folder"),tool("open_version",&p.id,"Open version folder","cube")));
    }
    if s.config.profiles.is_empty() {
        out.push_str(&empty(
            "profiles",
            "Your library starts here",
            "Create your first instance using the form beside this panel.",
            String::new(),
        ));
    }
    if let Ok(p) = s.config.profile() {
        out.push_str(&format!(
            "<div class=\"actions instance-folders\">{}{}</div>",
            button(
                "open_profile",
                &p.id,
                &format!("{} Instance folder", icon("folder")),
                "secondary small"
            ),
            button(
                "open_version",
                &p.id,
                &format!("{} Version folder", icon("cube")),
                "secondary small"
            )
        ));
        out.push_str(&format!("<section class=\"panel instance-editor\"><div class=\"panel-heading\"><h2>Instance settings</h2>{}</div><form data-action=\"edit_profile\">{}{}<label>Java override<span class=\"field-note\">Optional. Leave empty to use global Java.</span><input name=\"java\" value=\"{}\" placeholder=\"Use global Java\"></label><div class=\"actions\">{}{}{}</div></form><p class=\"note\">Removing an instance from this list keeps its worlds and files.</p></section>",badge(&p.name),field("Instance name","name",&p.name,"text"),field("Memory allocation (MiB)","memory",&p.memory_mb.to_string(),"number"),e(&p.java),submit("Save changes"),tool("install","","Install or repair instance","download"),tool("remove_profile",&p.id,"Remove instance from library","close")));
    }
    let versions = s
        .manifest
        .versions
        .iter()
        .filter(|v| v.kind == "release" || (s.config.settings.snapshots && v.kind == "snapshot"))
        .map(|v| {
            (
                v.id.clone(),
                format!(
                    "{}{}",
                    v.id,
                    if v.kind == "snapshot" {
                        " · snapshot"
                    } else {
                        ""
                    }
                ),
            )
        })
        .collect::<Vec<_>>();
    out.push_str(&format!("</section><section class=\"panel create-instance\"><div class=\"panel-heading\"><h2>New instance</h2><span class=\"subtle-icon\">{}</span></div><p>One version, one neatly organized home.</p><form data-action=\"create_profile\">{}<label>Minecraft version{}</label><label>Mod loader{}</label><label>Memory (MiB)<input type=\"number\" name=\"memory\" min=\"1024\" max=\"32768\" value=\"{}\" required></label>{}</form><div class=\"inline-info\">{}<p>Choose Fabric to install mods from Modrinth.</p></div></section></div>",icon("plus"),field("Instance name","name","","text"),select(versions,"version",&s.manifest.latest.release),select(vec![("vanilla".into(),"Vanilla".into()),("fabric".into(),"Fabric".into())],"loader","vanilla"),s.config.settings.memory_mb,submit(&format!("{} Create instance",icon("plus"))),icon("mods")));
    out
}
fn compact_count(n: u64) -> String {
    if n >= 1_000_000 {
        format!("{:.1}M", n as f64 / 1_000_000.0)
    } else if n >= 1000 {
        format!("{:.1}k", n as f64 / 1000.0)
    } else {
        n.to_string()
    }
}
fn modrinth(s: &State) -> String {
    let mut out = heading(
        "Discover mods",
        "A better world, one mod at a time.",
        badge("Powered by Modrinth"),
    );
    let Ok(p) = s.config.profile() else {
        return out
            + &empty(
                "mods",
                "Choose an instance first",
                "Mod search is tailored to your instance and Minecraft version.",
                button("navigate", "profiles", "Choose an instance", "primary"),
            );
    };
    if p.loader != "fabric" {
        return out
            + &empty(
                "mods",
                "Made for Fabric",
                "Create a Fabric instance to discover compatible Modrinth mods.",
                button("navigate", "profiles", "Manage instances", "primary"),
            );
    }
    out.push_str(&format!("<div class=\"discover-toolbar\"><form class=\"search-form\" data-action=\"search_mods\"><div class=\"search-input\">{}<input aria-label=\"Search Modrinth mods\" name=\"query\" value=\"{}\" placeholder=\"Search mods, authors, or a little inspiration…\"></div>{}</form><div class=\"filter-chips\">{}{}</div></div>",icon("search"),e(&s.query),submit("Search"),badge(&p.version),badge("Fabric")));
    if !s.mod_versions.is_empty() {
        let art = mod_art(s, &s.mod_project, &s.mod_title);
        out.push_str(&format!("<section class=\"version-panel\"><div class=\"version-heading\">{art}<div class=\"grow\"><h2>{}</h2><p>Compatible versions for Minecraft {} / Fabric</p></div>{}</div><form data-action=\"install_mod\"><label>Version{}</label>{}</form><p class=\"note\">Required dependencies are included. Optional dependencies stay optional.</p></section>",e(&s.mod_title),e(&p.version),tool("close_mod_details","","Close version picker","close"),select(s.mod_versions.iter().map(|v|(v.id.clone(),format!("{} · {}",v.version_number,v.version_type))),"version",&s.mod_versions[0].id),submit(&format!("{} Install mod",icon("download")))));
    }
    if !s.hits.is_empty() {
        out.push_str(&format!("<div class=\"results-summary\"><span><strong>{}</strong> compatible mods</span><span>Sorted by relevance</span></div><div class=\"mod-grid\">",s.mod_total));
        for h in &s.hits {
            let installed = p.mods.iter().any(|m| m.project == h.project_id);
            out.push_str(&format!("<article class=\"mod-card\"><div class=\"mod-card-header\">{}<div class=\"grow\"><h3>{}</h3><span class=\"mod-author\">by {}</span></div>{}</div><p class=\"mod-description\">{}</p><div class=\"mod-card-footer\"><span>{} {} downloads</span>{}</div></article>",mod_art(s,&h.project_id,&h.title),e(&h.title),e(&h.author),if installed{badge("Installed")}else{String::new()},e(&h.description),icon("download"),compact_count(h.downloads),button("mod_details",&h.project_id,if installed{"Versions"}else{"View mod"},"secondary small")));
        }
        out.push_str("</div>");
    } else {
        out.push_str(&empty(
            "search",
            "Find your next favorite",
            "Performance, new biomes, or a small quality-of-life upgrade. Search to explore.",
            String::new(),
        ));
    }
    if s.mod_total > 0 {
        out.push_str(&format!(
            "<div class=\"pagination\">{}<span>Page {} of {}</span>{}</div>",
            if s.mod_offset > 0 {
                button(
                    "mods_prev",
                    "",
                    &format!("{} Previous", icon("back")),
                    "quiet small",
                )
            } else {
                String::new()
            },
            s.mod_offset / 12 + 1,
            s.mod_total.div_ceil(12),
            if s.mod_offset + 12 < s.mod_total {
                button(
                    "mods_next",
                    "",
                    &format!("Next {}", icon("arrow")),
                    "quiet small",
                )
            } else {
                String::new()
            }
        ));
    }
    out
}
fn installed(s: &State) -> String {
    let mut out = heading(
        "Installed mods",
        "Fine-tune your selected instance.",
        button(
            "refresh_installed",
            "",
            &format!("{} Refresh", icon("refresh")),
            "secondary small",
        ) + &button(
            "navigate",
            "modrinth",
            &format!("{} Add mods", icon("plus")),
            "primary small",
        ),
    );
    let Ok(p) = s.config.profile() else {
        return out
            + &empty(
                "mods",
                "No instance selected",
                "Choose an instance to see and manage its mods.",
                button("navigate", "profiles", "Choose instance", "primary"),
            );
    };
    out.push_str(&format!("<div class=\"list-summary\"><span>{}</span><span>{} mods <span>·</span> {} enabled</span></div>",badge(&p.name),s.mod_counts(p).0,s.mod_counts(p).1));
    for m in &p.mods {
        out.push_str(&format!("<article class=\"installed-row {}\">{}<div class=\"grow\"><h3>{}</h3><p title=\"{}\">{}</p></div><div class=\"actions mod-actions\">{}{}<button type=\"button\" class=\"toggle\" role=\"switch\" aria-checked=\"{}\" title=\"{} {}\" aria-label=\"{} {}\" data-action=\"toggle_mod\" data-value=\"{}\"><span></span></button></div></article>",if m.enabled{""}else{"disabled-mod"},mod_art(s,&m.project,&m.title),e(&m.title),e(&m.filename),e(&m.filename),tool("update_mod",&m.project,"Check compatible versions","refresh"),tool("remove_mod",&m.project,"Move mod to removed-mods folder","close"),m.enabled,if m.enabled{"Disable"}else{"Enable"},e(&m.title),if m.enabled{"Disable"}else{"Enable"},e(&m.title),e(&m.project)));
    }
    if s.manual_mods_profile == p.id {
        for m in &s.manual_mods {
            out.push_str(&format!("<article class=\"installed-row {}\"><div class=\"mod-art\">{}</div><div class=\"grow\"><h3>{} {}</h3><p title=\"{}\">{}</p></div><div class=\"actions mod-actions\"><button type=\"button\" class=\"toggle\" role=\"switch\" aria-checked=\"{}\" title=\"{} {}\" aria-label=\"{} {}\" data-action=\"toggle_manual_mod\" data-value=\"{}\"><span></span></button></div></article>",if m.enabled { "" } else { "disabled-mod" },icon("mods"),e(m.filename.trim_end_matches(".disabled").trim_end_matches(".jar")),badge("Manual"),e(&m.filename),e(&m.filename),m.enabled,if m.enabled { "Disable" } else { "Enable" },e(&m.filename),if m.enabled { "Disable" } else { "Enable" },e(&m.filename),e(&m.filename)));
        }
        if !s.manual_mods_error.is_empty() {
            out.push_str(&format!(
                "<p class=\"note\" role=\"alert\">{}</p>",
                e(&s.manual_mods_error)
            ));
        }
    }
    if s.mod_counts(p).0 == 0 {
        out.push_str(&empty(
            "mods",
            "A clean canvas",
            "Install mods from Modrinth or copy JAR files into this instance’s mods folder.",
            button("navigate", "modrinth", "Discover mods", "secondary"),
        ));
    }
    out.push_str("<p class=\"note\">Manual JARs are detected automatically. Disabling adds .disabled; enabling restores .jar. Manual dependencies are not resolved automatically. Removed managed mods are kept in removed-mods.</p>");
    out
}
fn accounts(s: &State) -> String {
    let mut out = heading(
        "Accounts",
        "Your name. Your worlds. Your choice.",
        String::new(),
    );
    out.push_str("<div class=\"account-layout\"><section class=\"panel\"><div class=\"panel-heading\"><h2>Offline players</h2><span class=\"subtle-icon\">");
    out.push_str(&icon("accounts"));
    out.push_str("</span></div><p>For single-player and servers that allow offline accounts.</p>");
    for a in &s.config.offline_accounts {
        let active = s.session.as_ref().is_some_and(|v| v.uuid == a.uuid);
        out.push_str(&format!("<div class=\"player-row\"><div class=\"player-avatar\">{}</div><div class=\"grow\"><h3>{}</h3><small>Offline account</small></div><div class=\"actions\">{}{}</div></div>",e(&a.name.chars().take(1).collect::<String>()),e(&a.name),button("select_account",&a.uuid,if active{"Selected"}else{"Select"},if active{"selected-button small"}else{"secondary small"}),tool("remove_account",&a.uuid,"Remove offline player","close")));
    }
    out.push_str(&format!("<form class=\"add-player\" data-action=\"add_offline\"><label>Player name<input name=\"name\" minlength=\"3\" maxlength=\"16\" pattern=\"[A-Za-z0-9_]+\" placeholder=\"e.g. Steve\" required><span class=\"field-note\">3–16 letters, numbers, or underscores.</span></label>{}</form></section><section class=\"panel microsoft-panel\"><div class=\"panel-heading\"><h2>Microsoft account</h2><div class=\"microsoft-mark\" aria-hidden=\"true\"><i></i><i></i><i></i><i></i></div></div><p>Use your Java Edition identity on authenticated servers.</p>",submit(&format!("{} Add offline player",icon("plus")))));
    if !s.device_code.is_empty() {
        out.push_str(&format!("<div class=\"device-code\">{}</div><p>Enter this code on the Microsoft sign-in page.</p>{}",e(&s.device_code),button("open_signin","",&format!("{} Open Microsoft sign-in",icon("external")),"primary")));
    } else if let Some(a) = s.session.as_ref().filter(|a| !a.offline) {
        out.push_str(&format!(
            "<div class=\"online-player\"><span class=\"live-dot\"></span><h3>{}</h3></div>{}",
            e(&a.name),
            button("sign_out", "", "Sign out", "secondary")
        ));
    } else {
        out.push_str(&button("sign_in", "", "Continue with Microsoft", "primary"));
    }
    out.push_str(&format!("<div class=\"inline-info\">{}<p>Requires your own registered OAuth client ID. Tokens stay in memory; sign in again after restarting.</p></div>{}</section></div>",icon("shield"),button("navigate","settings",&format!("OAuth settings {}",icon("arrow")),"text-button")));
    out
}
fn settings(s: &State) -> String {
    let v = &s.config.settings;
    format!("{}<form class=\"settings-form\" data-action=\"save_settings\"><section class=\"settings-section\"><div class=\"settings-description\"><span class=\"section-number\">01</span><h2>Java &amp; storage</h2><p>Give each version the runtime it needs.</p></div><div class=\"settings-fields\"><div class=\"field-with-button\">{}{}</div><div class=\"field-with-button\">{}{}</div><p class=\"note\">Changing storage does not move existing game files.</p></div></section><section class=\"settings-section\"><div class=\"settings-description\"><span class=\"section-number\">02</span><h2>Game defaults</h2><p>Memory defaults apply to new instances.</p></div><div class=\"settings-fields\"><div class=\"grid three\">{}{}{}</div><label class=\"check-row\"><span>Launch in fullscreen</span><input type=\"checkbox\" name=\"fullscreen\" {}></label></div></section><section class=\"settings-section\"><div class=\"settings-description\"><span class=\"section-number\">03</span><h2>Downloads</h2><p>A little control over the essentials.</p></div><div class=\"settings-fields\">{}<label class=\"check-row\"><span>Show snapshot versions</span><input type=\"checkbox\" name=\"snapshots\" {}></label></div></section><section class=\"settings-section\"><div class=\"settings-description\"><span class=\"section-number\">04</span><h2>Microsoft OAuth</h2><p>Optional. Offline accounts work without it.</p></div><div class=\"settings-fields\"><label>Registered client ID<input name=\"client_id\" value=\"{}\" placeholder=\"Your public-client UUID\"></label><p class=\"note\">A public client with device flow and XboxLive.signin access is required.</p></div></section>{}<div class=\"settings-save\">{}{}</div></form>",heading("Settings","Small details. A smoother experience.",String::new()),field("Java executable","java",&v.java,"text"),tool("choose_java","","Choose Java executable","folder"),field("Game storage folder","game_root",&v.game_root.to_string_lossy(),"text"),tool("choose_root","","Choose storage folder","folder"),field("Memory (MiB)","memory",&v.memory_mb.to_string(),"number"),field("Width (px)","width",&v.width.to_string(),"number"),field("Height (px)","height",&v.height.to_string(),"number"),if v.fullscreen{"checked"}else{""},field("Concurrent downloads","workers",&v.workers.to_string(),"number"),if v.snapshots{"checked"}else{""},e(&v.client_id),discord_settings(s),submit("Save settings"),button("check_java","",&format!("{} Check Java",icon("check")),"secondary"))
}

fn discord_settings(s: &State) -> String {
    let d = &s.config.settings.discord;
    format!("<section class=\"settings-section\"><div class=\"settings-description\"><span class=\"section-number\">05</span><h2>Discord Rich Presence</h2><p>Your launcher and Minecraft activity.</p></div><div class=\"settings-fields\"><label class=\"check-row\"><span>Share activity on Discord</span><input type=\"checkbox\" name=\"discord_enabled\" {}></label><label>Discord Application ID<input name=\"discord_application_id\" value=\"{}\" inputmode=\"numeric\" placeholder=\"1558468312172732539\"></label><label>Artwork asset key<span class=\"field-note\">Optional. Upload the app icon under this key in your Discord application.</span><input name=\"discord_image_key\" value=\"{}\" placeholder=\"blockhaven\"></label><p class=\"note\">The Blockhaven Application ID is provided by default. You can replace it with your own. The desktop Discord client must be running. No bot token or login is needed.</p><p class=\"note\" role=\"status\" id=\"discord-status\">{}</p></div></section>", if d.enabled { "checked" } else { "" },e(&d.application_id),e(&d.image_key),e(&s.discord_status))
}
