import { useState, useEffect, useRef } from "preact/hooks";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import "./App.css";

interface FetchedLink {
  name: string;
  url: string;
  checked: boolean;
}

interface SavedLink {
  name: string;
  url: string;
  downloaded: boolean;
}

interface DownloadProgress {
  status: "started" | "progress" | "done" | "error";
  filename: string;
  total_mb: number;
  downloaded_mb: number;
  percent: number;
  speed_kbps: number;
  message?: string;
}

function formatSpeed(kbps: number): string {
  if (kbps >= 1024) return `${(kbps / 1024).toFixed(1)} MB/s`;
  return `${kbps.toFixed(0)} KB/s`;
}

function App() {
  const [url, setUrl] = useState("");
  const [browserPath, setBrowserPath] = useState("");
  const [fetchedLinks, setFetchedLinks] = useState<FetchedLink[]>([]);
  const [savedLinks, setSavedLinks] = useState<SavedLink[]>([]);
  const [status, setStatus] = useState("");
  const [loading, setLoading] = useState(false);
  const [saving, setSaving] = useState(false);
  const [processingIndex, setProcessingIndex] = useState<number | null>(null);

  // Auto-queue state
  const [autoRunning, setAutoRunning] = useState(false);
  const autoStopRef = useRef(false);

  // Real-time download progress from Tauri event
  const [dlProgress, setDlProgress] = useState<DownloadProgress | null>(null);

  useEffect(() => {
    invoke("load_config").then((res: any) => {
      if (res?.browser_path) setBrowserPath(res.browser_path);
    }).catch(console.error);
    refreshSavedList();

    // Subscribe to download-progress events from Rust
    const unlisten = listen<DownloadProgress>("download-progress", (event) => {
      setDlProgress(event.payload);
      if (event.payload.status === "done" || event.payload.status === "error") {
        // Auto-clear after 5s
        setTimeout(() => setDlProgress(null), 5000);
      }
    });

    return () => { unlisten.then(fn => fn()); };
  }, []);

  async function refreshSavedList() {
    try {
      const saved = await invoke<SavedLink[]>("load_links");
      setSavedLinks(saved);
    } catch (e) { console.error(e); }
  }

  async function getLink() {
    if (!url) { setStatus("Please enter a url."); return; }
    setLoading(true);
    setStatus("Scraping...");
    setFetchedLinks([]);    // Reset kolom kiri
    setSavedLinks([]);      // Reset kolom kanan juga
    // Kosongkan config list agar fresh setiap fetch baru
    await invoke("save_links", { links: [] }).catch(console.error);

    try {
      const bp = browserPath.trim() || null;
      await invoke("save_config", { browserPath: bp });

      const resp = await invoke("scrape_links", { url, browserPath: bp });
      const parsed: { href: string; text: string }[] = JSON.parse(resp as string);

      const newFetched: FetchedLink[] = parsed.map(p => ({
        name: p.text.trim() || p.href,
        url: p.href,
        checked: false,
      }));

      setFetchedLinks(newFetched);
      setStatus(newFetched.length > 0 ? `${newFetched.length} link ditemukan` : "Tidak ada link.");
    } catch (e) {
      setStatus("Error: " + String(e));
    } finally {
      setLoading(false);
    }
  }

  async function saveSelectedLinks() {
    const toAdd = fetchedLinks.filter(l => l.checked);
    if (toAdd.length === 0) { setStatus("Pilih link terlebih dahulu!"); return; }

    setSaving(true);
    try {
      const current = await invoke<SavedLink[]>("load_links");
      const existingUrls = new Set(current.map(l => l.url));
      const newOnes = toAdd
        .filter(l => !existingUrls.has(l.url))
        .map(l => ({ name: l.name, url: l.url, downloaded: false }));

      const merged = [...current, ...newOnes];
      await invoke("save_links", { links: merged });
      setSavedLinks(merged);

      const added = newOnes.length, skipped = toAdd.length - added;
      setStatus(added > 0
        ? `✅ ${added} ditambahkan${skipped > 0 ? `, ${skipped} sudah ada` : ""}`
        : `ℹ️ Semua sudah tersimpan`
      );
      setTimeout(() => setStatus(""), 3000);
    } catch (e) {
      setStatus("Gagal: " + String(e));
    } finally {
      setSaving(false);
    }
  }

  function toggleCheck(idx: number) {
    setFetchedLinks(prev => prev.map((l, i) => i === idx ? { ...l, checked: !l.checked } : l));
  }

  function toggleCheckAll() {
    const allChecked = fetchedLinks.every(l => l.checked);
    setFetchedLinks(prev => prev.map(l => ({ ...l, checked: !allChecked })));
  }

  // DL satu item manual — tampilkan popup saat selesai
  async function startDownload(idx: number) {
    const link = savedLinks[idx];
    setProcessingIndex(idx);
    try {
      const bp = browserPath.trim() || null;
      const resp = await invoke<string>("process_link", { url: link.url, browserPath: bp });

      const updated = savedLinks.map((l, i) => i === idx ? { ...l, downloaded: true } : l);
      setSavedLinks(updated);
      await invoke("save_links", {
        links: updated.map(l => ({ name: l.name, url: l.url, downloaded: l.downloaded })),
      });

      // Tampilkan popup hanya untuk download manual
      alert(resp);
    } catch (e) {
      alert("Error: " + String(e));
    } finally {
      setProcessingIndex(null);
    }
  }

  // Auto DL Queue — loop semua yang belum downloaded
  async function startAutoQueue() {
    autoStopRef.current = false;
    setAutoRunning(true);

    // Ambil list terbaru dari state
    const list = [...savedLinks];
    for (let i = 0; i < list.length; i++) {
      if (autoStopRef.current) break;
      if (list[i].downloaded) continue;

      setProcessingIndex(i);
      try {
        const bp = browserPath.trim() || null;
        await invoke("process_link", { url: list[i].url, browserPath: bp });

        // Setelah selesai (event "done" sudah di-emit oleh Rust), mark downloaded
        await invoke("mark_downloaded", { url: list[i].url });

        // Update state lokal
        setSavedLinks(prev => {
          const updated = prev.map((l, idx) => idx === i ? { ...l, downloaded: true } : l);
          return updated;
        });
      } catch (e) {
        console.error(`Error pada link ${i}:`, e);
        // Lanjutkan ke link berikutnya meski ada error
      }
    }

    setProcessingIndex(null);
    setAutoRunning(false);
    autoStopRef.current = false;
    await refreshSavedList();
  }

  function stopAutoQueue() {
    autoStopRef.current = true;
  }

  async function removeFromSaved(idx: number) {
    const updated = savedLinks.filter((_, i) => i !== idx);
    setSavedLinks(updated);
    await invoke("save_links", {
      links: updated.map(l => ({ name: l.name, url: l.url, downloaded: l.downloaded })),
    });
  }

  const allChecked = fetchedLinks.length > 0 && fetchedLinks.every(l => l.checked);
  const someChecked = fetchedLinks.some(l => l.checked);
  const checkedCount = fetchedLinks.filter(l => l.checked).length;
  const pendingCount = savedLinks.filter(l => !l.downloaded).length;

  return (
    <main class="container">
      <h1 class="title">FitDownloader</h1>

      {/* Form Panel */}
      <div class="glass-panel" style={{ marginBottom: '1.5rem' }}>
        <form onSubmit={(e) => { e.preventDefault(); getLink(); }}>
          <div class="input-group">
            <input
              id="url-input"
              onInput={(e) => setUrl(e.currentTarget.value)}
              placeholder="Paste FitGirl repack URL here..."
              value={url}
              required
            />
            <button class="btn-primary" type="submit" disabled={loading}>
              {loading ? (
                <span style={{ display: 'flex', alignItems: 'center', gap: '8px' }}>
                  <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" style={{ animation: 'spin 1.2s linear infinite' }}><line x1="12" y1="2" x2="12" y2="6"></line><line x1="12" y1="18" x2="12" y2="22"></line><line x1="4.93" y1="4.93" x2="7.76" y2="7.76"></line><line x1="16.24" y1="16.24" x2="19.07" y2="19.07"></line><line x1="2" y1="12" x2="6" y2="12"></line><line x1="18" y1="12" x2="22" y2="12"></line><line x1="4.93" y1="19.07" x2="7.76" y2="16.24"></line><line x1="16.24" y1="7.76" x2="19.07" y2="4.93"></line></svg>
                  Scraping...
                </span>
              ) : (
                <span style={{ display: 'flex', alignItems: 'center', gap: '8px' }}>
                  <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><path d="M21 15v4a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2v-4"></path><polyline points="7 10 12 15 17 10"></polyline><line x1="12" y1="15" x2="12" y2="3"></line></svg>
                  Fetch Links
                </span>
              )}
            </button>
          </div>
          <div class="input-group" style={{ marginBottom: 0 }}>
            <div style={{ position: 'relative', width: '100%' }}>
              <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" style={{ position: 'absolute', left: '12px', top: '14px', color: '#8b949e' }}><circle cx="12" cy="12" r="3"></circle><path d="M19.4 15a1.65 1.65 0 0 0 .33 1.82l.06.06a2 2 0 0 1 0 2.83 2 2 0 0 1-2.83 0l-.06-.06a1.65 1.65 0 0 0-1.82-.33 1.65 1.65 0 0 0-1 1.51V21a2 2 0 0 1-2 2 2 2 0 0 1-2-2v-.09A1.65 1.65 0 0 0 9 19.4a1.65 1.65 0 0 0-1.82.33l-.06.06a2 2 0 0 1-2.83 0 2 2 0 0 1 0-2.83l.06-.06a1.65 1.65 0 0 0 .33-1.82 1.65 1.65 0 0 0-1.51-1H3a2 2 0 0 1-2-2 2 2 0 0 1 2-2h.09A1.65 1.65 0 0 0 4.6 9a1.65 1.65 0 0 0-.33-1.82l-.06-.06a2 2 0 0 1 0-2.83 2 2 0 0 1 2.83 0l.06.06a1.65 1.65 0 0 0 1.82.33H9a1.65 1.65 0 0 0 1-1.51V3a2 2 0 0 1 2-2 2 2 0 0 1 2 2v.09a1.65 1.65 0 0 0 1 1.51 1.65 1.65 0 0 0 1.82-.33l.06-.06a2 2 0 0 1 2.83 0 2 2 0 0 1 0 2.83l-.06.06a1.65 1.65 0 0 0-.33 1.82V9a1.65 1.65 0 0 0 1.51 1H21a2 2 0 0 1 2 2 2 2 0 0 1-2 2h-.09a1.65 1.65 0 0 0-1.51 1z"></path></svg>
              <input
                id="browser-input"
                onInput={(e) => setBrowserPath(e.currentTarget.value)}
                placeholder="Custom Chromium binary path (Optional)"
                value={browserPath}
                style={{ paddingLeft: '38px' }}
              />
            </div>
          </div>
        </form>
      </div>

      {/* Download Progress Bar */}
      {dlProgress && (
        <div class={`progress-banner ${dlProgress.status}`}>
          <div class="progress-info">
            <span class="progress-filename">{dlProgress.filename}</span>
            <span class="progress-meta">
              {dlProgress.status === "done" && "✅ Selesai!"}
              {dlProgress.status === "error" && `❌ Error: ${dlProgress.message}`}
              {dlProgress.status === "progress" && (
                <>
                  {dlProgress.downloaded_mb.toFixed(1)} / {dlProgress.total_mb.toFixed(1)} MB
                  {" · "}
                  <strong>{formatSpeed(dlProgress.speed_kbps)}</strong>
                </>
              )}
              {dlProgress.status === "started" && "⬇ Memulai download..."}
            </span>
          </div>
          {(dlProgress.status === "progress" || dlProgress.status === "done") && (
            <div class="progress-bar-wrap">
              <div class="progress-bar-fill" style={{ width: `${dlProgress.percent}%` }}></div>
            </div>
          )}
        </div>
      )}

      {/* Two-column layout */}
      <div class="columns-layout">

        {/* ── KIRI: Fetched Links ── */}
        <div class="glass-panel col-panel">
          <div class="result-header">
            <h2>
              <svg width="15" height="15" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" style={{ marginRight: '8px', verticalAlign: 'middle' }}><path d="M21 15v4a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2v-4"></path><polyline points="7 10 12 15 17 10"></polyline><line x1="12" y1="15" x2="12" y2="3"></line></svg>
              Fetched
            </h2>
            {status && (
              <span class={`status-badge ${loading ? 'loading' : status.startsWith('Error') ? 'error' : ''}`}>
                {status}
              </span>
            )}
          </div>

          {fetchedLinks.length > 0 && (
            <div class="list-toolbar">
              <label class="check-all-label" onClick={toggleCheckAll}>
                <div class={`custom-checkbox ${allChecked ? 'checked' : someChecked ? 'indeterminate' : ''}`}>
                  {allChecked && <svg width="10" height="10" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="3"><polyline points="20 6 9 17 4 12"></polyline></svg>}
                  {!allChecked && someChecked && <svg width="10" height="10" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="3"><line x1="5" y1="12" x2="19" y2="12"></line></svg>}
                </div>
                <span class="check-all-text">Select All</span>
              </label>
              {someChecked && <span class="selected-count">{checkedCount} selected</span>}
              <button
                class="btn-clear"
                onClick={() => setFetchedLinks([])}
                title="Hapus semua list kiri"
              >
                🗑 Clear
              </button>
              <button
                class="btn-save"
                onClick={saveSelectedLinks}
                disabled={saving || !someChecked}
                style={{ marginLeft: 'auto' }}
              >
                {saving ? '💾 Saving...' : `💾 Save (${checkedCount})`}
              </button>
            </div>
          )}

          <div class="links-container">
            {fetchedLinks.length > 0 ? (
              fetchedLinks.map((link, idx) => (
                <div class={`link-item ${savedLinks.some(s => s.url === link.url) ? 'already-saved' : ''}`} key={idx}>
                  <div class={`custom-checkbox ${link.checked ? 'checked' : ''}`} onClick={() => toggleCheck(idx)} style={{ flexShrink: 0 }}>
                    {link.checked && <svg width="10" height="10" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="3"><polyline points="20 6 9 17 4 12"></polyline></svg>}
                  </div>
                  <div class="link-info">
                    <span class="link-text"><span class="link-index">#{idx + 1}</span> {link.name}</span>
                  </div>
                  {savedLinks.some(s => s.url === link.url) && (
                    <span class="badge-saved" title="Sudah di queue">✓</span>
                  )}
                </div>
              ))
            ) : (
              <div class="empty-state">
                <svg width="40" height="40" viewBox="0 0 24 24" fill="none" stroke="rgba(139,148,158,0.35)" stroke-width="1"><path d="M21 15v4a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2v-4"></path><polyline points="17 8 12 3 7 8"></polyline><line x1="12" y1="3" x2="12" y2="15"></line></svg>
                <p>Paste URL dan klik Fetch</p>
              </div>
            )}
          </div>
        </div>

        {/* ── KANAN: Download Queue ── */}
        <div class="glass-panel col-panel">
          <div class="result-header">
            <h2>
              <svg width="15" height="15" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" style={{ marginRight: '8px', verticalAlign: 'middle' }}><line x1="8" y1="6" x2="21" y2="6"></line><line x1="8" y1="12" x2="21" y2="12"></line><line x1="8" y1="18" x2="21" y2="18"></line><line x1="3" y1="6" x2="3.01" y2="6"></line><line x1="3" y1="12" x2="3.01" y2="12"></line><line x1="3" y1="18" x2="3.01" y2="18"></line></svg>
              Queue
            </h2>
            {savedLinks.length > 0 && (
              <span class="status-badge">
                {savedLinks.filter(l => l.downloaded).length}/{savedLinks.length} done
              </span>
            )}
          </div>

          {/* Auto Queue toolbar */}
          {savedLinks.length > 0 && pendingCount > 0 && (
            <div class="list-toolbar">
              {!autoRunning ? (
                <button class="btn-auto-all" onClick={startAutoQueue} disabled={processingIndex !== null}>
                  <svg width="13" height="13" viewBox="0 0 24 24" fill="currentColor"><polygon points="5 3 19 12 5 21 5 3"></polygon></svg>
                  Auto DL All ({pendingCount} pending)
                </button>
              ) : (
                <button class="btn-stop" onClick={stopAutoQueue}>
                  <svg width="13" height="13" viewBox="0 0 24 24" fill="currentColor"><rect x="6" y="6" width="12" height="12"></rect></svg>
                  Stop Queue
                </button>
              )}
              {autoRunning && processingIndex !== null && (
                <span class="status-badge loading" style={{ marginLeft: '8px' }}>
                  Processing #{processingIndex + 1}...
                </span>
              )}
            </div>
          )}

          <div class="links-container">
            {savedLinks.length > 0 ? (
              savedLinks.map((link, idx) => (
                <div class={`link-item ${link.downloaded ? 'downloaded' : ''} ${processingIndex === idx ? 'processing' : ''}`} key={idx}>
                  <div class="link-info">
                    <a href={link.url} target="_blank" rel="noopener noreferrer" class="link-text">
                      <span class="link-index">#{idx + 1}</span> {link.name}
                    </a>
                  </div>
                  <div style={{ display: 'flex', alignItems: 'center', gap: '6px', flexShrink: 0 }}>
                    {link.downloaded ? (
                      <span class="badge-done">
                        <svg width="10" height="10" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="3"><polyline points="20 6 9 17 4 12"></polyline></svg>
                        Done
                      </span>
                    ) : (
                      <button
                        class="btn-action"
                        onClick={() => startDownload(idx)}
                        disabled={processingIndex !== null || autoRunning}
                        title="Download satu item ini"
                      >
                        {processingIndex === idx ? (
                          <span style={{ display: 'flex', alignItems: 'center', gap: '5px' }}>
                            <span class="dot-pulse"></span> DL...
                          </span>
                        ) : (
                          <span style={{ display: 'flex', alignItems: 'center', gap: '5px' }}>
                            <svg width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5"><polygon points="5 3 19 12 5 21 5 3"></polygon></svg>
                            DL
                          </span>
                        )}
                      </button>
                    )}
                    <button
                      class="btn-remove"
                      onClick={() => removeFromSaved(idx)}
                      disabled={processingIndex === idx || autoRunning}
                      title="Hapus dari queue"
                    >
                      <svg width="11" height="11" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5"><line x1="18" y1="6" x2="6" y2="18"></line><line x1="6" y1="6" x2="18" y2="18"></line></svg>
                    </button>
                  </div>
                </div>
              ))
            ) : (
              <div class="empty-state">
                <svg width="40" height="40" viewBox="0 0 24 24" fill="none" stroke="rgba(139,148,158,0.35)" stroke-width="1"><rect x="3" y="3" width="18" height="18" rx="2"></rect><line x1="9" y1="9" x2="15" y2="15"></line><line x1="15" y1="9" x2="9" y2="15"></line></svg>
                <p>Pilih link di kiri, klik Save</p>
              </div>
            )}
          </div>
        </div>

      </div>
    </main>
  );
}

export default App;
