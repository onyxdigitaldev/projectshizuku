const { invoke } = window.__TAURI__.core;
const { listen } = window.__TAURI__.event;

// --- State ---
let currentView = 'home';
let previousView = 'home';
let currentAnime = null;
let currentShowId = null;
let currentEpisodes = [];
let currentSources = [];
let currentSourceIndex = 0;
let activeGenre = null;
let quitModalVisible = false;
let genrePage = 1;
let genreLoading = false;
let genreHasMore = true;
let downloadRefreshInterval = null;
let passwordCallback = null;

const GENRES = [
    'Action', 'Adventure', 'Comedy', 'Drama', 'Fantasy', 'Horror',
    'Mecha', 'Music', 'Mystery', 'Romance', 'Sci-Fi', 'Slice of Life',
    'Sports', 'Supernatural', 'Thriller',
];

// --- DOM refs ---
const $ = (s) => document.querySelector(s);
const $$ = (s) => document.querySelectorAll(s);

// --- Navigation ---
function showView(name) {
    previousView = currentView;
    ['home-view', 'detail-view', 'player-view', 'downloads-view', 'history-view', 'settings-view', 'mokuroku-view'].forEach(v => {
        $(`#${v}`).classList.add('hidden');
    });
    $('#search-overlay').classList.add('hidden');

    if (name === 'search') {
        $('#search-overlay').classList.remove('hidden');
        $('#search-input').focus();
    } else {
        $(`#${name}-view`).classList.remove('hidden');
    }

    // Stop download refresh when leaving downloads view
    if (name !== 'downloads') stopDownloadRefresh();

    currentView = name;
    updateStatus();
}

// Fix 1: remove error/stalled listeners before clearing video src
function goBack() {
    if (currentView === 'player') {
        const video = $('#video-player');
        video.removeEventListener('error', video._onError);
        video.removeEventListener('stalled', video._onStalled);
        video.pause();
        video.src = '';
        showView('detail');
    } else if (currentView === 'search') {
        showView('home');
    } else if (currentView === 'detail') {
        showView('home');
    } else if (['downloads', 'history', 'settings', 'mokuroku'].includes(currentView)) {
        showView('home');
    } else if (currentView === 'home' && activeGenre) {
        clearGenre();
    }
}

function updateStatus() {
    const map = {
        home: activeGenre ? `genre: ${activeGenre}` : 'home',
        search: 'search',
        detail: currentAnime?.title_english || currentAnime?.title_romaji || '',
        player: 'playing',
        downloads: 'downloads',
        history: 'history',
        settings: 'settings',
        mokuroku: 'mokuroku',
    };
    const el = $('#status-line');
    el.textContent = `[${map[currentView] || currentView}]`;
    el.classList.remove('warning');
}

function setLoading(on) {
    $('#loading').classList.toggle('hidden', !on);
}

// Fix 6: warning helper
function showWarning(msg) {
    const el = $('#status-line');
    el.textContent = `[warn: ${msg}]`;
    el.classList.add('warning');
    setTimeout(() => {
        el.classList.remove('warning');
        updateStatus();
    }, 5000);
}

// --- Card rendering ---
function renderCard(anime, onClick) {
    const title = anime.title_english || anime.title_romaji || 'Unknown';
    const score = anime.average_score ? `${anime.average_score}%` : '';
    const status = anime.status || '';
    const statusClass = status === 'FINISHED' ? 'finished' : '';
    const cover = anime.cover_image || '';

    const card = document.createElement('div');
    card.className = 'card';
    card.tabIndex = 0;
    card.dataset.animeId = anime.id;
    card.innerHTML = `
        <img class="card-image" src="${cover}" alt="${escapeHtml(title)}" loading="lazy" onerror="this.style.display='none'" />
        <div class="card-body">
            <div class="card-title">${escapeHtml(title)}</div>
            <div class="card-meta">
                ${score ? `<span class="card-score">${score}</span>` : ''}
                ${status ? ` <span class="card-status ${statusClass}">${status.toLowerCase()}</span>` : ''}
            </div>
        </div>
    `;
    const handler = onClick || (() => openAnimeDetail(anime));
    card.addEventListener('click', handler);
    card.addEventListener('keydown', (e) => { if (e.key === 'Enter') handler(); });
    return card;
}

function renderGrid(container, animeList) {
    container.innerHTML = '';
    if (!animeList || animeList.length === 0) {
        container.innerHTML = '<div class="empty-state">no results found</div>';
        return;
    }
    animeList.forEach(a => container.appendChild(renderCard(a)));
}

// --- Home ---
async function loadHome() {
    setLoading(true);
    try {
        const [trending, updated] = await Promise.all([
            invoke('get_trending'),
            invoke('get_recently_updated'),
        ]);
        renderGrid($('#trending-grid'), trending);
        renderGrid($('#updated-grid'), updated);
    } catch (e) {
        console.error('Failed to load home:', e);
        $('#trending-grid').innerHTML = `<div class="empty-state">failed to load: ${escapeHtml(String(e))}</div>`;
    }
    setLoading(false);
    initGenrePills();
}

// --- Genre browsing (Fix 2: pagination) ---
function initGenrePills() {
    const container = $('#genre-pills');
    container.innerHTML = '';
    GENRES.forEach(genre => {
        const pill = document.createElement('button');
        pill.className = 'genre-pill';
        pill.textContent = genre.toLowerCase();
        pill.addEventListener('click', () => browseGenre(genre));
        container.appendChild(pill);
    });
}

async function browseGenre(genre) {
    activeGenre = genre;
    genrePage = 1;
    genreHasMore = true;
    $$('.genre-pill').forEach(p => p.classList.toggle('active', p.textContent === genre.toLowerCase()));
    $('#genre-results').classList.remove('hidden');
    $('#home-default').classList.add('hidden');
    $('#genre-results-title').textContent = `// ${genre.toLowerCase()}`;
    $('#genre-results-grid').innerHTML = '';
    updateStatus();
    await loadGenrePage(genre, 1);
}

async function loadGenrePage(genre, page) {
    if (genreLoading || !genreHasMore) return;
    genreLoading = true;
    setLoading(true);
    try {
        const results = await invoke('browse_genre', { genre, page });
        if (!results || results.length === 0) {
            genreHasMore = false;
            if (page === 1) {
                $('#genre-results-grid').innerHTML = '<div class="empty-state">no results found</div>';
            }
        } else {
            results.forEach(a => $('#genre-results-grid').appendChild(renderCard(a)));
            if (results.length < 30) genreHasMore = false;
        }
    } catch (e) {
        if (page === 1) {
            $('#genre-results-grid').innerHTML = `<div class="empty-state">failed to load: ${escapeHtml(String(e))}</div>`;
        }
    }
    genreLoading = false;
    setLoading(false);
}

// Infinite scroll for genre browsing
$('#app').addEventListener('scroll', () => {
    if (!activeGenre || !genreHasMore || genreLoading) return;
    const app = $('#app');
    if (app.scrollTop + app.clientHeight >= app.scrollHeight - 200) {
        genrePage++;
        loadGenrePage(activeGenre, genrePage);
    }
});

function clearGenre() {
    activeGenre = null;
    genrePage = 1;
    genreHasMore = true;
    genreLoading = false;
    $$('.genre-pill').forEach(p => p.classList.remove('active'));
    $('#genre-results').classList.add('hidden');
    $('#home-default').classList.remove('hidden');
    updateStatus();
}

// --- Search ---
let searchTimeout = null;

async function performSearch(query) {
    if (!query.trim()) {
        $('#search-results').innerHTML = '';
        return;
    }
    setLoading(true);
    try {
        const results = await invoke('search_anime', { query: query.trim() });
        renderGrid($('#search-results'), results);
    } catch (e) {
        $('#search-results').innerHTML = `<div class="empty-state">search failed: ${escapeHtml(String(e))}</div>`;
    }
    setLoading(false);
}

$('#search-input').addEventListener('input', (e) => {
    clearTimeout(searchTimeout);
    searchTimeout = setTimeout(() => performSearch(e.target.value), 400);
});

$('#search-input').addEventListener('keydown', (e) => {
    if (e.key === 'Escape') {
        showQuitModal();
        e.preventDefault();
    } else if (e.key === 'q' && !e.target.value) {
        showView('home');
        e.preventDefault();
    } else if (e.key === 'Enter') {
        const firstCard = $('#search-results .card');
        if (firstCard) firstCard.focus();
    }
});

// --- Detail view ---
async function openAnimeDetail(anime) {
    currentAnime = anime;
    showView('detail');

    const title = anime.title_english || anime.title_romaji || 'Unknown';
    const romaji = anime.title_romaji || '';

    $('#detail-title').textContent = title;
    $('#detail-subtitle').textContent = romaji !== title ? romaji : '';
    $('#detail-cover').src = anime.cover_image || '';

    if (anime.banner_image) {
        $('#detail-banner').style.backgroundImage = `url(${anime.banner_image})`;
        $('#detail-banner').style.display = '';
    } else {
        $('#detail-banner').style.display = 'none';
    }

    $('#detail-tags').innerHTML = (anime.genres || [])
        .map(g => `<span class="tag">${escapeHtml(g)}</span>`)
        .join('');

    const desc = (anime.description || 'No description available.')
        .replace(/<br\s*\/?>/gi, '\n')
        .replace(/<[^>]+>/g, '');
    $('#detail-description').textContent = desc;

    updateMokurokuBadge(anime.id);

    renderRelated([]);
    try {
        const fullAnime = await invoke('get_anime_detail', { id: anime.id });
        if (fullAnime.relations && fullAnime.relations.length > 0) {
            renderRelated(fullAnime.relations);
        }
    } catch (e) {
        console.error('Failed to fetch relations:', e);
    }

    $('#episode-list').innerHTML = '<div class="empty-state">loading episodes...</div>';
    try {
        const providerResults = await invoke('provider_search', {
            query: romaji || title,
            mode: 'sub',
        });
        if (providerResults.length > 0) {
            currentShowId = providerResults[0].id;
            const episodes = await invoke('get_episodes', {
                showId: currentShowId,
                mode: 'sub',
            });
            currentEpisodes = episodes;
            renderEpisodes(episodes);
        } else {
            $('#episode-list').innerHTML = '<div class="empty-state">no sources found for this anime</div>';
        }
    } catch (e) {
        $('#episode-list').innerHTML = `<div class="empty-state">failed to load episodes: ${escapeHtml(String(e))}</div>`;
    }
}

// --- Related series rendering ---
function renderRelated(relations) {
    const sections = {
        seasons: { types: ['SEQUEL', 'PREQUEL'], el: 'related-seasons', row: 'related-seasons-row' },
        movies: { types: ['MOVIE'], formatMatch: true, el: 'related-movies', row: 'related-movies-row' },
        ovas: { types: ['OVA', 'SPECIAL', 'ONA'], formatMatch: true, el: 'related-ovas', row: 'related-ovas-row' },
        spinoffs: { types: ['SIDE_STORY', 'SPIN_OFF'], el: 'related-spinoffs', row: 'related-spinoffs-row' },
        other: { types: ['ALTERNATIVE', 'CHARACTER', 'SOURCE', 'COMPILATION', 'CONTAINS'], el: 'related-other', row: 'related-other-row' },
    };

    $('#detail-related').classList.add('hidden');
    Object.values(sections).forEach(s => {
        $(`#${s.el}`).classList.add('hidden');
        $(`#${s.row}`).innerHTML = '';
    });

    if (!relations || relations.length === 0) return;

    let hasAny = false;

    Object.entries(sections).forEach(([key, sec]) => {
        let items;
        if (sec.formatMatch) {
            items = relations.filter(r => sec.types.includes(r.format || '') || sec.types.includes(r.relation_type));
        } else {
            items = relations.filter(r => sec.types.includes(r.relation_type));
        }

        if (key === 'seasons') {
            items.sort((a, b) => (a.season_year || 0) - (b.season_year || 0));
        }

        if (items.length > 0) {
            hasAny = true;
            $(`#${sec.el}`).classList.remove('hidden');
            const row = $(`#${sec.row}`);
            items.forEach(rel => {
                const card = document.createElement('div');
                card.className = 'related-card';
                const relTitle = rel.title_english || rel.title_romaji || 'Unknown';
                const meta = [rel.format, rel.season_year, rel.episodes ? `${rel.episodes} ep` : ''].filter(Boolean).join(' · ');
                card.innerHTML = `
                    <img class="related-card-image" src="${rel.cover_image || ''}" alt="${escapeHtml(relTitle)}" loading="lazy" onerror="this.style.display='none'" />
                    <div class="related-card-body">
                        <div class="related-card-title">${escapeHtml(relTitle)}</div>
                        <div class="related-card-meta">${escapeHtml(meta)}</div>
                    </div>
                `;
                card.addEventListener('click', async () => {
                    try {
                        const fullAnime = await invoke('get_anime_detail', { id: rel.id });
                        openAnimeDetail(fullAnime);
                    } catch (e) {
                        console.error('Failed to open related:', e);
                    }
                });
                row.appendChild(card);
            });
        }
    });

    if (hasAny) {
        $('#detail-related').classList.remove('hidden');
    }
}

// --- Episode rendering (Fix 3: download buttons) ---
function renderEpisodes(episodes) {
    const container = $('#episode-list');
    container.innerHTML = '';
    if (!episodes.length) {
        container.innerHTML = '<div class="empty-state">no episodes available</div>';
        return;
    }
    episodes.forEach((ep) => {
        const wrapper = document.createElement('div');
        wrapper.className = 'episode-item';

        const btn = document.createElement('button');
        btn.className = 'episode-btn';
        btn.textContent = `Ep ${ep}`;
        btn.tabIndex = 0;
        btn.addEventListener('click', () => playEpisode(ep));

        const dlBtn = document.createElement('button');
        dlBtn.className = 'episode-dl-btn';
        dlBtn.textContent = 'dl';
        dlBtn.title = 'download episode';
        dlBtn.tabIndex = 0;
        dlBtn.addEventListener('click', (e) => {
            e.stopPropagation();
            downloadEpisode(ep, determineBarrel());
        });

        wrapper.appendChild(btn);
        wrapper.appendChild(dlBtn);
        container.appendChild(wrapper);
    });
}

// Fix 3: determine barrel name from current anime context
function determineBarrel() {
    if (!currentAnime) return '';
    const format = currentAnime.format;
    if (format === 'MOVIE') return 'Movies';
    if (format === 'OVA' || format === 'ONA' || format === 'SPECIAL') return 'OVAs';
    if (currentAnime.season && currentAnime.season_year) {
        return `${currentAnime.season_year} ${currentAnime.season}`;
    }
    return 'Season 1';
}

// Fix 3: barrel download — download all episodes
async function barrelDownload() {
    if (!currentAnime || !currentShowId || currentEpisodes.length === 0) return;

    const title = currentAnime.title_english || currentAnime.title_romaji || '';
    const barrel = determineBarrel();
    const total = currentEpisodes.length;

    $('#status-line').textContent = `[barrel download: ${total} episodes queuing...]`;

    for (let i = 0; i < currentEpisodes.length; i++) {
        const ep = currentEpisodes[i];
        try {
            await downloadEpisode(ep, barrel);
        } catch (e) {
            console.error(`Barrel download failed for ep ${ep}:`, e);
        }
        $('#status-line').textContent = `[barrel: queued ${i + 1}/${total}]`;
    }
    $('#status-line').textContent = `[barrel download: all ${total} episodes queued]`;
    setTimeout(updateStatus, 3000);
}

// Wire barrel download button
$('#barrel-download-btn').addEventListener('click', barrelDownload);

// --- Mokuroku badge ---
async function updateMokurokuBadge(animeId) {
    try {
        const inList = await invoke('is_in_mokuroku', { animeId });
        $('#mokuroku-badge').classList.toggle('hidden', !inList);
        $('#mokuroku-add').classList.toggle('hidden', inList);
    } catch {
        $('#mokuroku-badge').classList.add('hidden');
        $('#mokuroku-add').classList.remove('hidden');
    }
}

async function toggleMokuroku() {
    if (!currentAnime) return;
    const title = currentAnime.title_english || currentAnime.title_romaji || '';
    try {
        const inList = await invoke('is_in_mokuroku', { animeId: currentAnime.id });
        if (inList) {
            await invoke('remove_from_mokuroku', { animeId: currentAnime.id });
            $('#status-line').textContent = `[removed from mokuroku]`;
        } else {
            await invoke('add_to_mokuroku', {
                animeId: currentAnime.id,
                title,
                coverImage: currentAnime.cover_image || null,
            });
            $('#status-line').textContent = `[added to mokuroku]`;
        }
        updateMokurokuBadge(currentAnime.id);
        setTimeout(updateStatus, 2000);
    } catch (e) {
        console.error('Mokuroku toggle failed:', e);
    }
}

// --- Mokuroku view ---
async function loadMokuroku() {
    showView('mokuroku');
    setLoading(true);
    try {
        const list = await invoke('get_mokuroku');
        const container = $('#mokuroku-grid');
        container.innerHTML = '';
        if (!list.length) {
            container.innerHTML = '<div class="empty-state">mokuroku is empty — add anime with [m] from the detail page</div>';
            setLoading(false);
            return;
        }
        list.forEach(entry => {
            const fakeAnime = {
                id: entry.anime_id,
                title_english: entry.title,
                title_romaji: null,
                cover_image: entry.cover_image,
                status: null,
                average_score: null,
            };
            container.appendChild(renderCard(fakeAnime, async () => {
                try {
                    const fullAnime = await invoke('get_anime_detail', { id: entry.anime_id });
                    openAnimeDetail(fullAnime);
                } catch (e) {
                    console.error('Failed to open from mokuroku:', e);
                }
            }));
        });
    } catch (e) {
        $('#mokuroku-grid').innerHTML = '<div class="empty-state">failed to load mokuroku</div>';
    }
    setLoading(false);
}

// --- Player with auto-cycling + mpv fallback ---
async function playEpisode(epNumber) {
    if (!currentShowId) return;
    setLoading(true);

    const title = currentAnime?.title_english || currentAnime?.title_romaji || '';

    // Check for local file first
    try {
        const localPath = await invoke('check_local_file', {
            animeId: currentAnime?.id || 0,
            episode: String(epNumber),
        });
        if (localPath) {
            showView('player');
            $('#player-title').textContent = `${title} - Episode ${epNumber} [local]`;
            const video = $('#video-player');
            video.src = `file://${localPath}`;
            video.play().catch(() => {});
            setLoading(false);
            invoke('add_to_history', { animeId: currentAnime?.id || 0, title, episode: String(epNumber) }).catch(() => {});
            return;
        }
    } catch {}

    try {
        const sources = await invoke('get_episode_sources', {
            showId: currentShowId,
            episode: String(epNumber),
            mode: 'sub',
        });

        if (sources.length === 0) {
            setLoading(false);
            $('#status-line').textContent = '[error: no sources found]';
            setTimeout(updateStatus, 3000);
            return;
        }

        currentSources = [
            ...sources.filter(s => s.url.includes('.mp4')),
            ...sources.filter(s => !s.url.includes('.mp4')),
        ];
        currentSourceIndex = 0;

        showView('player');
        $('#player-title').textContent = `${title} - Episode ${epNumber}`;
        trySource(title, epNumber);

        invoke('add_to_history', {
            animeId: currentAnime?.id || 0,
            title,
            episode: String(epNumber),
        }).catch(() => {});

    } catch (e) {
        console.error('Play failed:', e);
        $('#status-line').textContent = `[error: ${e}]`;
        setTimeout(updateStatus, 3000);
    }
    setLoading(false);
}

function trySource(title, epNumber) {
    if (currentSourceIndex >= currentSources.length) {
        fallbackToMpv(title, epNumber);
        return;
    }

    const source = currentSources[currentSourceIndex];
    const total = currentSources.length;
    const current = currentSourceIndex + 1;

    // Fix 6: warn about non-mp4 formats
    if (!source.url.includes('.mp4')) {
        $('#status-line').textContent = `[source ${current}/${total}: ${source.provider} ${source.quality} (non-mp4)]`;
        $('#status-line').classList.add('warning');
    } else {
        $('#status-line').textContent = `[source ${current}/${total}: ${source.provider} ${source.quality}]`;
    }

    const video = $('#video-player');
    video.removeEventListener('error', video._onError);
    video.removeEventListener('stalled', video._onStalled);

    video._onError = () => {
        currentSourceIndex++;
        trySource(title, epNumber);
    };

    let stalledTimer = null;
    video._onStalled = () => {
        if (stalledTimer) clearTimeout(stalledTimer);
        stalledTimer = setTimeout(() => {
            currentSourceIndex++;
            trySource(title, epNumber);
        }, 10000);
    };

    video.addEventListener('error', video._onError);
    video.addEventListener('stalled', video._onStalled);
    video.addEventListener('playing', () => {
        if (stalledTimer) clearTimeout(stalledTimer);
        $('#status-line').textContent = `[playing: ${source.provider} ${source.quality}]`;
        $('#status-line').classList.remove('warning');
    }, { once: true });

    video.src = source.url;
    video.play().catch(() => {
        currentSourceIndex++;
        trySource(title, epNumber);
    });
}

// Fix 6: warn on mpv fallback
async function fallbackToMpv(title, epNumber) {
    if (currentSources.length === 0) {
        showWarning('no playable sources found');
        return;
    }
    showWarning('embedded player failed — launching mpv');
    const source = currentSources[0];
    try {
        await invoke('play_in_mpv', {
            url: source.url,
            title: `${title} - Episode ${epNumber}`,
            referrer: null,
        });
        $('#status-line').textContent = '[playing in mpv]';
        setTimeout(() => showView('detail'), 1000);
    } catch (e) {
        showWarning(`mpv failed: ${e}`);
    }
}

// --- Downloads (Fix 4: barrel param, Fix 6: format warning) ---
async function downloadEpisode(epNumber, barrel = '') {
    if (!currentAnime || !currentShowId) return;
    const title = currentAnime.title_english || currentAnime.title_romaji || '';
    try {
        const format = await invoke('start_download', {
            showId: currentShowId,
            animeId: currentAnime.id,
            animeTitle: title,
            episode: String(epNumber),
            mode: 'sub',
            barrel: barrel,
        });
        if (format === 'm3u8') {
            showWarning(`ep${epNumber}: m3u8 stream — download via yt-dlp/ffmpeg, no progress`);
        } else {
            $('#status-line').textContent = `[downloading: ${title} ep${epNumber}]`;
            setTimeout(updateStatus, 2000);
        }
    } catch (e) {
        console.error('Download failed:', e);
        $('#status-line').textContent = `[download error: ${e}]`;
        setTimeout(updateStatus, 3000);
    }
}

// Fix 5: hierarchical downloads view with auto-refresh
async function loadDownloads() {
    showView('downloads');
    try {
        const downloads = await invoke('get_downloads');
        renderDownloadList(downloads);
    } catch (e) {
        $('#download-list').innerHTML = '<div class="empty-state">failed to load downloads</div>';
    }
    startDownloadRefresh();
}

function renderDownloadList(downloads) {
    const container = $('#download-list');
    container.innerHTML = '';
    if (!downloads.length) {
        container.innerHTML = '<div class="empty-state">no downloads — press [dl] on an episode or [dl all] for a barrel</div>';
        return;
    }

    // Group by anime_id, then by barrel
    const groups = {};
    downloads.forEach(dl => {
        const key = dl.anime_id;
        if (!groups[key]) groups[key] = { title: dl.title.split(' - ')[0], barrels: {} };
        const barrelKey = dl.barrel || '';
        if (!groups[key].barrels[barrelKey]) groups[key].barrels[barrelKey] = [];
        groups[key].barrels[barrelKey].push(dl);
    });

    Object.entries(groups).forEach(([animeId, group]) => {
        const seriesHeader = document.createElement('div');
        seriesHeader.className = 'download-series-header';
        seriesHeader.textContent = group.title;
        container.appendChild(seriesHeader);

        Object.entries(group.barrels).forEach(([barrelName, items]) => {
            if (barrelName) {
                const barrelHeader = document.createElement('div');
                barrelHeader.className = 'download-barrel-header';
                barrelHeader.textContent = barrelName;
                container.appendChild(barrelHeader);
            }

            items.forEach(dl => {
                const item = document.createElement('div');
                item.className = 'download-item';
                item.dataset.downloadId = dl.id;
                const pct = Math.round(dl.progress * 100);
                const statusColor = dl.status === 'complete' ? 'var(--green)' : dl.status === 'failed' ? 'var(--red)' : 'var(--text-dim)';
                const epLabel = `Ep ${dl.episode}`;
                item.innerHTML = `
                    <span class="download-title">${escapeHtml(epLabel)}</span>
                    <div class="download-progress">
                        <div class="download-progress-bar" style="width: ${pct}%"></div>
                    </div>
                    <span class="download-status" style="color: ${statusColor}">${dl.status} ${dl.status === 'downloading' ? pct + '%' : ''}</span>
                `;
                container.appendChild(item);
            });
        });
    });
}

// Fix 8: auto-refresh downloads view
function startDownloadRefresh() {
    stopDownloadRefresh();
    downloadRefreshInterval = setInterval(async () => {
        if (currentView !== 'downloads') { stopDownloadRefresh(); return; }
        try {
            const downloads = await invoke('get_downloads');
            if (downloads.some(dl => dl.status === 'downloading' || dl.status === 'queued')) {
                renderDownloadList(downloads);
            }
        } catch {}
    }, 3000);
}

function stopDownloadRefresh() {
    if (downloadRefreshInterval) { clearInterval(downloadRefreshInterval); downloadRefreshInterval = null; }
}

// Fix 8: handle unknown total size in progress events
listen('download-progress', (event) => {
    const { id, progress, downloaded } = event.payload;
    const item = $(`.download-item[data-download-id="${id}"]`);
    if (item) {
        const bar = item.querySelector('.download-progress-bar');
        const status = item.querySelector('.download-status');
        if (progress >= 0) {
            if (bar) bar.style.width = `${Math.round(progress * 100)}%`;
            if (status) status.textContent = `downloading ${Math.round(progress * 100)}%`;
        } else {
            if (bar) bar.style.width = '50%';
            const mb = (downloaded / 1048576).toFixed(1);
            if (status) status.textContent = `downloading ${mb} MB`;
        }
        if (status) status.style.color = 'var(--text-dim)';
    }
});

listen('download-complete', (event) => {
    const { id } = event.payload;
    const item = $(`.download-item[data-download-id="${id}"]`);
    if (item) {
        const bar = item.querySelector('.download-progress-bar');
        const status = item.querySelector('.download-status');
        if (bar) bar.style.width = '100%';
        if (status) {
            status.textContent = 'complete';
            status.style.color = 'var(--green)';
        }
    }
});

listen('download-failed', (event) => {
    const { id } = event.payload;
    const item = $(`.download-item[data-download-id="${id}"]`);
    if (item) {
        const status = item.querySelector('.download-status');
        if (status) {
            status.textContent = 'failed';
            status.style.color = 'var(--red)';
        }
    }
});

// --- History ---
async function loadHistory() {
    showView('history');
    try {
        const history = await invoke('get_history');
        const container = $('#history-list');
        container.innerHTML = '';
        if (!history.length) {
            container.innerHTML = '<div class="empty-state">no watch history yet</div>';
            return;
        }
        history.forEach(([animeId, title, episode, date]) => {
            const item = document.createElement('div');
            item.className = 'history-item';
            item.tabIndex = 0;
            item.innerHTML = `
                <span class="history-title">${escapeHtml(title)}</span>
                <span class="history-episode">ep ${escapeHtml(episode)}</span>
                <span class="history-date">${escapeHtml(date)}</span>
            `;
            container.appendChild(item);
        });
    } catch (e) {
        $('#history-list').innerHTML = '<div class="empty-state">failed to load history</div>';
    }
}

// --- Settings ---
async function loadSettings() {
    showView('settings');
    try {
        const settings = await invoke('get_settings');
        const container = $('#settings-list');
        container.innerHTML = '';

        const adultItem = document.createElement('div');
        adultItem.className = 'settings-item';
        adultItem.tabIndex = 0;
        const isAdult = settings.allow_adult === 'true';
        adultItem.innerHTML = `
            <span class="settings-label">adult content</span>
            <span class="settings-value ${isAdult ? 'enabled' : ''}">${isAdult ? 'enabled' : 'disabled'}</span>
            <span class="settings-hint">[Enter] toggle (requires password)</span>
        `;
        adultItem.addEventListener('click', toggleAdult);
        adultItem.addEventListener('keydown', (e) => { if (e.key === 'Enter') toggleAdult(); });
        container.appendChild(adultItem);
    } catch (e) {
        $('#settings-list').innerHTML = '<div class="empty-state">failed to load settings</div>';
    }
}

// Fix 7: in-app password auth instead of pkexec
async function toggleAdult() {
    try {
        const hasPassword = await invoke('has_adult_password');
        if (!hasPassword) {
            showPasswordModal('set adult content password', true, async (pw) => {
                try {
                    await invoke('set_adult_password', { password: pw });
                    const result = await invoke('toggle_adult_content', { password: pw });
                    $('#status-line').textContent = `[adult content: ${result ? 'enabled' : 'disabled'}]`;
                    hidePasswordModal();
                    loadSettings();
                    loadHome();
                } catch (e) {
                    showPasswordError(String(e));
                }
            });
        } else {
            showPasswordModal('enter password to toggle adult content', false, async (pw) => {
                try {
                    const result = await invoke('toggle_adult_content', { password: pw });
                    $('#status-line').textContent = `[adult content: ${result ? 'enabled' : 'disabled'}]`;
                    hidePasswordModal();
                    loadSettings();
                    loadHome();
                } catch (e) {
                    if (String(e).includes('incorrect')) {
                        showPasswordError('incorrect password');
                    } else {
                        showPasswordError(String(e));
                    }
                }
            });
        }
    } catch (e) {
        $('#status-line').textContent = `[error: ${e}]`;
        setTimeout(updateStatus, 3000);
    }
}

// --- Password modal ---
function showPasswordModal(title, needConfirm, callback) {
    passwordCallback = callback;
    $('#password-modal-title').textContent = title;
    $('#password-input').value = '';
    $('#password-confirm').value = '';
    $('#password-modal-error').classList.add('hidden');
    $('#password-modal-confirm-row').classList.toggle('hidden', !needConfirm);
    $('#password-modal').classList.remove('hidden');
    $('#password-input').focus();
}

function hidePasswordModal() {
    passwordCallback = null;
    $('#password-modal').classList.add('hidden');
}

function showPasswordError(msg) {
    $('#password-modal-error').textContent = msg;
    $('#password-modal-error').classList.remove('hidden');
}

// --- Quit modal ---
function showQuitModal() {
    quitModalVisible = true;
    $('#quit-modal').classList.remove('hidden');
}

function hideQuitModal() {
    quitModalVisible = false;
    $('#quit-modal').classList.add('hidden');
}

// --- Keyboard navigation ---
document.addEventListener('keydown', (e) => {
    // Password modal takes priority
    if (!$('#password-modal').classList.contains('hidden')) {
        if (e.key === 'Escape') {
            hidePasswordModal();
        } else if (e.key === 'Enter') {
            const pw = $('#password-input').value;
            const confirm = $('#password-confirm').value;
            if (!$('#password-modal-confirm-row').classList.contains('hidden')) {
                if (pw !== confirm) { showPasswordError('passwords do not match'); e.preventDefault(); return; }
                if (pw.length < 4) { showPasswordError('password too short (min 4 chars)'); e.preventDefault(); return; }
            }
            if (!pw) { showPasswordError('password required'); e.preventDefault(); return; }
            if (passwordCallback) passwordCallback(pw);
        }
        e.preventDefault();
        return;
    }

    // Quit modal takes priority
    if (quitModalVisible) {
        if (e.key === 'y' || e.key === 'Y') {
            invoke('quit_app').catch(() => window.close());
        } else {
            hideQuitModal();
        }
        e.preventDefault();
        return;
    }

    // Don't capture when typing in search
    if (document.activeElement === $('#search-input') && currentView === 'search') {
        if (e.key === 'Escape') {
            showQuitModal();
            e.preventDefault();
        }
        return;
    }

    // Don't capture when in password fields
    if (document.activeElement === $('#password-input') || document.activeElement === $('#password-confirm')) {
        return;
    }

    switch (e.key) {
        case 'Escape':
            showQuitModal();
            e.preventDefault();
            break;
        case 'q':
            goBack();
            break;
        case '/':
            e.preventDefault();
            showView('search');
            break;
        case 'h':
            showView('home');
            clearGenre();
            break;
        case 'd':
            loadDownloads();
            break;
        case 'r':
            loadHistory();
            break;
        case 's':
            loadSettings();
            break;
        case 'w':
            loadMokuroku();
            break;
        case 'm':
            if (currentView === 'detail') toggleMokuroku();
            break;
        case 'f':
            if (currentView === 'player') {
                if (document.fullscreenElement) {
                    document.exitFullscreen();
                } else {
                    $('#video-player').requestFullscreen();
                }
            }
            break;
    }
});

// --- Util ---
function escapeHtml(text) {
    const div = document.createElement('div');
    div.textContent = text;
    return div.innerHTML;
}

// --- Init ---
loadHome();
updateStatus();
