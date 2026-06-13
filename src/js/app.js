// Tauri v2 IPC bridge
const invoke = window.__TAURI_INTERNALS__.invoke;
import { getSubscriptions, addSubscription, removeSubscription, updateSubscriptionAvatar, getSubscribedFeed, getChannelVideos, getChannelLive, getVideo, getSuggestions, searchChannels, getVideoUrl, getComments, getChannelAvatar, getVideoFormats, getThumbnails } from './api.js';
import { renderSubscriptionItem } from './subscriptions.js';

function debug(msg) {
  if (window.__TAURI_INTERNALS__) window.__TAURI_INTERNALS__.invoke('log_message', { msg }).catch(() => {});
  document.title = 'ToBe - ' + msg.substring(0, 80);
}
const VERSION = '0.1.6';
document.title = `ToBe v${VERSION}`;
debug(`ToBe v${VERSION} loaded`);

let settings = null;
let subscriptions = [];
let selectedChannel = null;
const videoCache = new Map();
const videoCacheTime = new Map();
const CACHE_TTL = 10 * 60 * 1000;
const $ = id => document.getElementById(id);

class PaginatedView {
  constructor(containerId, scrollViewId, opts = {}) {
    this.container = $(containerId); this.scrollEl = $(scrollViewId);
    this.pageSize = opts.pageSize || 20; this.fetchFn = opts.fetchFn;
    this.all = []; this.rendered = 0; this.loading = false;
    this.scrollEl.onscroll = () => { if (this.loading || this.rendered >= this.all.length) return; if (this.scrollEl.scrollHeight - this.scrollEl.scrollTop - this.scrollEl.clientHeight < 300) this.nextPage(); };
  }
  async load(reset = false) {
    if (this.loading) return; this.loading = true;
    if (reset) { this.all = []; this.rendered = 0; this.container.innerHTML = ''; }
    try { if (this.all.length === 0) { this.all = await this.fetchFn(); } this.loading = false; this.nextPage(); }
    catch (err) { debug(`[feed] error: ${err}`); } this.loading = false;
  }
  nextPage() {
    if (this.loading || this.rendered >= this.all.length) return; this.loading = true;
    const start = this.rendered; const end = Math.min(start + this.pageSize, this.all.length);
    const slice = this.all.slice(start, end); this.rendered = end;
    const frag = document.createDocumentFragment();
    for (let i = 0; i < slice.length; i++) frag.appendChild(createVideoCard(slice[i]));
    this.container.appendChild(frag); lazyLoadThumbnails(this.container); this.loading = false;
  }
}

let feedView;
function getFeedView() {
  if (!feedView) feedView = new PaginatedView('video-feed', 'feed-view', { pageSize: 50, fetchFn: async () => getSubscribedFeed(subscriptions, 1, settings.invidious_instance) });
  return feedView;
}

let channelVideosAll = [], channelVideosRenderCount = 0, channelVideosLoading = false, channelPage = 1;

function loadChannelVideosMore() {
  if (channelVideosLoading || channelVideosRenderCount >= channelVideosAll.length) return;
  channelVideosLoading = true;
  const start = channelVideosRenderCount; const end = Math.min(start + 10, channelVideosAll.length);
  const slice = channelVideosAll.slice(start, end); channelVideosRenderCount = end;
  const container = $('channel-videos');
  const frag = document.createDocumentFragment();
  for (let i = 0; i < slice.length; i++) frag.appendChild(createVideoCard(slice[i]));
  container.appendChild(frag); lazyLoadThumbnails(container); channelVideosLoading = false;
}

async function init() {
  debug('init started');
  try { settings = await invoke('get_settings'); applySettings(); subscriptions = await getSubscriptions(); renderSubscriptions(); showView('feed'); getFeedView().load(true); }
  catch (err) { debug('Init error: ' + err); }
}

function applySettings() {
  if (settings.theme === 'light') document.body.classList.add('light');
  $('setting-invidious').value = settings.invidious_instance; $('setting-sort').value = settings.default_sort;
  $('setting-theme').value = settings.theme; $('setting-autoplay').checked = settings.autoplay_next; $('sort-select').value = settings.default_sort;
}

function renderSubscriptions() {
  const list = $('subscription-list'); list.innerHTML = '';
  if (!subscriptions.length) { list.innerHTML = '<div class="loading">No subscriptions yet.<br>Search and add channels!</div>'; return; }
  list.innerHTML = subscriptions.map(s => renderSubscriptionItem(s)).join('');
  list.querySelectorAll('.subscription-item').forEach(el => { el.onclick = () => openChannel(el.dataset.id, el.querySelector('.subscription-name').textContent); });
}

async function openChannel(channelId, channelName) {
  showView('channel');
  const header = $('channel-header'); header.dataset.channelId = channelId;
  const isSubscribed = subscriptions.some(s => s.channel_id === channelId);
  header.innerHTML = `<div class="channel-avatar-large avatar-letter">${(channelName||'?')[0].toUpperCase()}</div><div class="channel-info"><h2>${escapeHtml(channelName)}</h2><span>Loading...</span></div>${isSubscribed ? `<button class="btn-unsubscribe" id="btn-unsubscribe">Unsubscribe</button>` : ''}`;
  if (isSubscribed) $('btn-unsubscribe').onclick = () => confirmUnsubscribe(channelId);
  loadChannelVideos(channelId);
  try { const ch = await invoke('get_channel', { channelId, invidiousInstance: settings.invidious_instance }); const hasAv = ch.channel_avatar && !ch.channel_avatar.includes('/channel/'); header.innerHTML = `${hasAv ? `<img class="channel-avatar-large" src="${ch.channel_avatar}" alt="" onerror="this.style.display='none'">` : `<div class="channel-avatar-large avatar-letter">${(ch.channel_name||'?')[0].toUpperCase()}</div>`}<div class="channel-info"><h2>${escapeHtml(ch.channel_name)}</h2><span>${ch.subscriber_count.toLocaleString()} subscribers</span></div>${isSubscribed ? `<button class="btn-unsubscribe" id="btn-unsubscribe">Unsubscribe</button>` : ''}`; if (isSubscribed) $('btn-unsubscribe').onclick = () => confirmUnsubscribe(channelId); if (isSubscribed && ch.channel_avatar && !ch.channel_avatar.includes('/channel/')) { const sub = subscriptions.find(s => s.channel_id === channelId); if (sub && sub.channel_avatar !== ch.channel_avatar) { sub.channel_avatar = ch.channel_avatar; renderSubscriptions(); } } } catch (e) {}
}

async function loadChannelVideos(channelId) {
  const container = $('channel-videos'); container.innerHTML = '<div class="loading">Loading videos...</div>';
  const cached = videoCache.get(channelId); const cachedAt = videoCacheTime.get(channelId) || 0;
  let videos;
  if (cached && Date.now() - cachedAt < CACHE_TTL) { videos = cached; }
  else { videos = await getChannelVideos(channelId, 1, settings.invidious_instance); videoCache.set(channelId, videos); videoCacheTime.set(channelId, Date.now()); channelPage = 2; }
  channelVideosAll = videos; channelVideosRenderCount = 0; channelVideosLoading = false;
  container.innerHTML = '';
  const firstBatch = Math.min(20, channelVideosAll.length);
  const frag = document.createDocumentFragment();
  for (let i = 0; i < firstBatch; i++) frag.appendChild(createVideoCard(channelVideosAll[i]));
  container.appendChild(frag); channelVideosRenderCount = firstBatch; lazyLoadThumbnails(container);
  const btn = document.createElement('div'); btn.id = 'channel-load-more'; btn.className = 'load-more-btn';
  btn.textContent = 'Load more'; btn.onclick = async () => {
    btn.textContent = 'Loading...'; btn.disabled = true;
    try {
      const more = await getChannelVideos(channelId, channelPage, settings.invidious_instance);
      if (more.length > 0) {
        channelVideosAll = [...channelVideosAll, ...more]; channelPage++;
        const f2 = document.createDocumentFragment();
        for (let i = 0; i < more.length; i++) f2.appendChild(createVideoCard(more[i]));
        container.insertBefore(f2, btn); lazyLoadThumbnails(container);
        btn.textContent = 'Load more'; btn.disabled = false;
      } else { btn.textContent = 'No more videos'; }
    } catch(e) { btn.textContent = 'Load more (error)'; btn.disabled = false; }
  };
  container.appendChild(btn);
}

async function confirmUnsubscribe(channelId) { if (!confirm('Unsubscribe from this channel?')) return; try { await removeSubscription(channelId); subscriptions = await getSubscriptions(); renderSubscriptions(); showView('feed'); } catch (err) { debug('unsubscribe error: ' + err); } }

async function loadChannelLive(channelId) {
  const container = $('channel-videos'); container.innerHTML = '<div class="loading">Loading live streams...</div>';
  try { const videos = await getChannelLive(channelId, settings.invidious_instance); container.innerHTML = ''; if (!videos.length) { container.innerHTML = '<div class="loading">No live streams</div>'; return; } const frag = document.createDocumentFragment(); for (let i = 0; i < videos.length; i++) frag.appendChild(createVideoCard(videos[i])); container.appendChild(frag); } catch (err) { container.innerHTML = '<div class="loading">Failed to load live streams</div>'; }
}

function escapeHtmlFast(str) { return str ? str.replace(/&/g,'&amp;').replace(/</g,'&lt;').replace(/>/g,'&gt;').replace(/"/g,'&quot;') : ''; }

function createVideoCard(video) {
  const el = document.createElement('div'); el.className = 'video-card'; el.dataset.videoId = video.video_id;
  const thumbUrl = video.thumbnail || '';
  const imgAttr = thumbUrl ? `data-src="${thumbUrl}" src="data:image/gif;base64,R0lGODlhAQABAIAAAAAAAP///yH5BAEAAAAALAAAAAABAAEAAAIBRAA7"` : '';
  const sub = subscriptions.find(s => s.channel_id === video.channel_id);
  const chAv = sub && sub.channel_avatar && !sub.channel_avatar.includes('/channel/') ? sub.channel_avatar : '';
  el.innerHTML = `<div class="video-thumbnail"><img ${imgAttr} alt="" loading="lazy">${video.is_live ? '<span class="video-live-badge">LIVE</span>' : ''}<span class="video-duration">${video.is_live ? 'LIVE' : formatDuration(video.duration)}</span></div><div class="video-card-info"><div class="video-card-title">${escapeHtmlFast(video.title)}</div><div class="video-card-meta"><span class="video-card-channel-avatar-wrap" data-channel-id="${video.channel_id}">${chAv ? `<img class="video-card-channel-avatar" src="${chAv}" alt="" onerror="this.style.display='none'">` : ''}</span><span>${escapeHtmlFast(video.channel_name)}</span> · ${formatDate(video.published_at)} · ${formatViews(video.view_count)} views</div></div>`;
  // If no avatar yet, fetch asynchronously and inject into the wrap
  if (!chAv && sub) {
    const cid = video.channel_id;
    getChannelAvatar(cid).then(url => {
      if (url && !url.includes('/channel/')) {
        sub.channel_avatar = url;
        updateSubscriptionAvatar(cid, url).catch(() => {});
        const wrap = document.querySelector(`.video-card-channel-avatar-wrap[data-channel-id="${cid}"]`);
        if (wrap && !wrap.querySelector('img')) {
          const img = document.createElement('img'); img.className = 'video-card-channel-avatar';
          img.src = url; img.alt = ''; img.onerror = () => img.style.display = 'none'; wrap.appendChild(img);
        }
      }
    }).catch(() => {});
  }
  el.onclick = () => playVideo(video.video_id); return el;
}

function lazyLoadThumbnails(container) {
  const imgs = container.querySelectorAll('img[data-src]'); if (!imgs.length) return;
  let i = 0;
  const batch = () => { const end = Math.min(i + 5, imgs.length); for (; i < end; i++) { if (imgs[i].dataset.src) { imgs[i].src = imgs[i].dataset.src; imgs[i].removeAttribute('data-src'); } } if (i < imgs.length) setTimeout(batch, 50); };
  setTimeout(batch, 100);
}

let currentVideoId = null, videoFormats = [], currentFormatId = null;

async function playVideo(videoId) {
  currentVideoId = videoId; showView('player');
  try {
    const player = $('video-player'); const playerInfo = $('video-info');
    player.pause(); player.removeAttribute('src'); player.load();
    playerInfo.innerHTML = '<div class="loading">Loading video...</div>'; $('suggestions-list').innerHTML = '';
    const [video, streamUrl] = await Promise.all([getVideo(videoId, settings.invidious_instance), getVideoUrl(videoId)]);
    if (currentVideoId !== videoId) return;
    player.src = streamUrl; player.play(); currentFormatId = null;
    getVideoFormats(videoId).then(fmts => { if (currentVideoId !== videoId) return; videoFormats = fmts.filter(f => f.height > 0); renderResolutionPicker(); }).catch(() => {});
    playerInfo.innerHTML = `<h2>${escapeHtml(video.title)}</h2><div class="video-channel-row" id="video-channel-row"><div class="video-channel-avatar" id="video-channel-avatar">${escapeHtml((video.channel_name||'?')[0].toUpperCase())}</div><div class="video-channel-name" id="video-channel-name">${escapeHtml(video.channel_name)}</div></div><div class="video-info-meta">${formatViews(video.view_count)} views · ${formatViews(video.like_count)} likes · ${formatDate(video.published_at)}</div><div class="video-description-collapsed" id="video-description">${escapeHtml(truncateText(video.description||'',250))}</div>${(video.description||'').length>250 ? `<button class="btn-expand" id="btn-expand-desc">Show more</button>` : ''}`;
    if (video.channel_id) getChannelAvatar(video.channel_id).then(url => { if (currentVideoId !== videoId) return; const av = document.getElementById('video-channel-avatar'); if (av && url) av.outerHTML = `<img class="video-channel-avatar" src="${escapeHtml(url)}" alt="" onerror="this.style.display='none">`; }).catch(() => {});
    const be = $('btn-expand-desc'); if (be) be.onclick = () => { $('video-description').textContent = escapeHtml(video.description||''); $('video-description').className = 'video-description-full'; be.remove(); };
    loadSuggestions(video.title); loadComments(videoId);
    player.onended = () => { if (currentVideoId === videoId && settings.autoplay_next) playNextSuggestion(); };
  } catch (err) { if (currentVideoId === videoId) debug('player error: ' + err); }
}

function renderResolutionPicker() {
  const container = $('video-player-container');
  let picker = $('resolution-picker');
  if (!picker) { picker = document.createElement('div'); picker.id = 'resolution-picker'; picker.className = 'resolution-picker'; container.appendChild(picker); }
  const best = videoFormats.length > 0 ? videoFormats[0] : null;
  let label = best ? best.note : 'Auto';
  if (currentFormatId) { const cur = videoFormats.find(f => f.format_id === currentFormatId); if (cur) label = cur.note; }
  picker.innerHTML = `<button class="res-btn" id="res-toggle">${escapeHtml(label)} ▾</button><div class="res-dropdown hidden" id="res-dropdown"><div class="res-option ${!currentFormatId ? 'res-active' : ''}" data-fid="">Auto (${best ? best.note : 'best'})</div>${videoFormats.map(f => `<div class="res-option ${currentFormatId === f.format_id ? 'res-active' : ''}" data-fid="${f.format_id}">${escapeHtml(f.note)}${f.filesize ? ` · ${(f.filesize/1024/1024).toFixed(1)}MB` : ''}</div>`).join('')}</div>`;
  $('res-toggle').onclick = (e) => { e.stopPropagation(); $('res-dropdown').classList.toggle('hidden'); };
  document.querySelectorAll('.res-option').forEach(el => { el.onclick = async () => { const fid = el.dataset.fid; $('res-dropdown').classList.add('hidden'); await switchResolution(fid); }; });
  document.addEventListener('click', () => { const dd = $('res-dropdown'); if (dd) dd.classList.add('hidden'); }, { once: true });
}

async function switchResolution(formatId) {
  const player = $('video-player'); if (!player || !currentVideoId) return; currentFormatId = formatId;
  try { const url = await getVideoUrl(currentVideoId, formatId || undefined); if (currentVideoId) { const wasPlaying = !player.paused; player.src = url; if (wasPlaying) player.play(); renderResolutionPicker(); } } catch (err) { debug('res switch error: ' + err); }
}

async function loadSuggestions(query) {
  const list = $('suggestions-list'); list.innerHTML = '<div class="loading">Loading...</div>';
  try {
    const suggestions = await getSuggestions(query, settings.invidious_instance); list.innerHTML = '';
    suggestions.forEach(s => {
      const el = document.createElement('div'); el.className = 'suggestion-item';
      const dur = s.duration > 0 ? formatDuration(s.duration) : '';
      const date = s.published_at > 0 ? formatDate(s.published_at) : '';
      const meta = [escapeHtml(s.channel_name), date].filter(Boolean).join(' · ');
      el.innerHTML = `<div class="suggestion-thumbnail"><img src="${s.thumbnail}" alt="" loading="lazy">${dur ? `<div class="suggestion-duration">${dur}</div>` : ''}</div><div class="suggestion-info"><div class="suggestion-title">${escapeHtml(s.title)}</div><div class="suggestion-channel">${meta}</div></div>`;
      el.onclick = () => playVideo(s.video_id); list.appendChild(el);
    });
  } catch (err) { list.innerHTML = '<div class="loading">No suggestions</div>'; }
}

function playNextSuggestion() { const l = $('suggestions-list'); const f = l.querySelector('.suggestion-item'); if (f) f.click(); }

async function loadComments(videoId, page = 1, reset = true) {
  const list = $('comments-list'); if (reset) list.innerHTML = '<div class="loading">Loading comments...</div>';
  try {
    const resp = await getComments(videoId, settings.invidious_instance); const all = resp.comments || [];
    const start = (page - 1) * 20; const slice = all.slice(start, start + 20); const hasMore = start + 20 < all.length;
    if (reset) list.innerHTML = '';
    slice.forEach(c => { const el = document.createElement('div'); el.className = 'comment-item'; const av = c.authorThumbnails && c.authorThumbnails.length > 0 ? c.authorThumbnails[c.authorThumbnails.length-1].url : ''; el.innerHTML = `<img class="comment-avatar" src="${av}" alt="" onerror="this.style.display='none'"><div class="comment-body"><div><span class="comment-author">${escapeHtml(c.author)}</span><span class="comment-time">${escapeHtml(c.publishedText)}</span></div><div class="comment-text">${escapeHtml(c.content)}</div><div class="comment-likes">${c.likeCount > 0 ? '👍 ' + c.likeCount.toLocaleString() : ''}</div></div>`; list.appendChild(el); });
    const existing = $('comments-load-more'); if (existing) existing.remove();
    if (hasMore) { const btn = document.createElement('div'); btn.id = 'comments-load-more'; btn.className = 'load-more-btn'; btn.textContent = `Load more comments (${start+slice.length}/${all.length})`; btn.onclick = () => loadComments(videoId, page + 1, false); list.appendChild(btn); }
  } catch (err) { if (reset) list.innerHTML = '<div class="loading">Comments unavailable</div>'; }
}

async function saveSettings() { settings.invidious_instance = $('setting-invidious').value.trim(); settings.default_sort = $('setting-sort').value; settings.theme = $('setting-theme').value; settings.autoplay_next = $('setting-autoplay').checked; await invoke('update_settings', { settings }); applySettings(); showView('feed'); }
function openModal() { $('modal-channel-input').value = ''; $('modal-search-results').innerHTML = ''; $('modal-overlay').classList.remove('hidden'); $('modal-channel-input').focus(); selectedChannel = null; $('btn-modal-add').disabled = true; }
function closeModal() { $('modal-overlay').classList.add('hidden'); selectedChannel = null; }
async function confirmAddSubscription() {
  let channelId, channelName, channelAvatar = '';
  if (selectedChannel) {
    channelId = selectedChannel.channel_id;
    channelName = selectedChannel.channel_name;
    channelAvatar = selectedChannel.channel_avatar || '';
  } else {
    const q = $('modal-channel-input').value.trim();
    if (!q) return;
    channelName = q;
    if (q.includes('/channel/')) {
      channelId = q.split('/channel/')[1].split(/[?/]/)[0];
    } else if (q.includes('@') || q.includes('/c/') || q.includes('/user/')) {
      try {
        const channels = await searchChannels(q, settings.invidious_instance);
        if (channels.length > 0) {
          channelId = channels[0].channel_id;
          channelName = channels[0].channel_name;
          channelAvatar = channels[0].channel_avatar || '';
        } else { return; }
      } catch (e) { return; }
    } else {
      channelId = q;
    }
  }
  closeModal();
  try {
    await invoke('add_subscription', {
      channel: { channel_id: channelId, channel_name: channelName, channel_avatar: channelAvatar, subscriber_count: 0, description: '' }
    });
    subscriptions = await getSubscriptions();
    renderSubscriptions();
  } catch (err) { debug('add subscription error: ' + err); }
}
async function searchChannel() {
  const q = $('modal-channel-input').value.trim();
  const results = $('modal-search-results');
  if (!q) { results.innerHTML = ''; return; }
  results.innerHTML = '<div class="loading">Searching...</div>';
  try {
    const channels = await searchChannels(q, settings.invidious_instance);
    if (!channels.length) { results.innerHTML = '<div class="loading">No channels found</div>'; return; }
    results.innerHTML = channels.map(ch =>
      `<div class="search-result" data-id="${escapeHtml(ch.channel_id)}" data-name="${escapeHtml(ch.channel_name)}" data-avatar="${escapeHtml(ch.channel_avatar||'')}"><img src="${ch.channel_avatar||''}" alt="" onerror="this.style.display='none'"><span>${escapeHtml(ch.channel_name)}</span></div>`
    ).join('');
    results.querySelectorAll('.search-result').forEach(el => {
      el.onclick = () => {
        document.querySelectorAll('.search-result').forEach(r => r.classList.remove('selected'));
        el.classList.add('selected');
        selectedChannel = {
          channel_id: el.dataset.id,
          channel_name: el.dataset.name,
          channel_avatar: el.dataset.avatar
        };
        $('modal-channel-input').value = el.dataset.name;
        $('btn-modal-add').disabled = false;
      };
    });
  } catch (err) { results.innerHTML = '<div class="loading">Search failed</div>'; }
}

function bindEvents() {
  $('btn-settings').onclick = () => showView('settings'); $('btn-back').onclick = () => showView('feed');
  $('btn-add-sub').onclick = openModal; $('btn-modal-cancel').onclick = closeModal; $('btn-modal-add').onclick = confirmAddSubscription; $('btn-save-settings').onclick = saveSettings;
  const bs = $('btn-all-subs'); if (bs) bs.onclick = () => { showView('feed'); getFeedView().load(true); };
  const si = $('modal-channel-input'); if (si) si.oninput = debounce(searchChannel, 500);
  $('sort-select').onchange = () => getFeedView().load(true);
  document.querySelectorAll('.channel-tabs .tab').forEach(tab => { tab.onclick = () => { document.querySelectorAll('.channel-tabs .tab').forEach(t => t.classList.remove('active')); tab.classList.add('active'); const cid = $('channel-header').dataset.channelId; if (tab.dataset.tab === 'live') loadChannelLive(cid); else loadChannelVideos(cid); }; });
  $('channel-view').onscroll = () => { if (channelVideosLoading || channelVideosRenderCount >= channelVideosAll.length) return; if ($('channel-view').scrollHeight - $('channel-view').scrollTop - $('channel-view').clientHeight < 300) loadChannelVideosMore(); };
}

function showView(v) { document.querySelectorAll('.view').forEach(x => { x.classList.remove('active'); x.classList.add('hidden'); }); $(v+'-view').classList.remove('hidden'); $(v+'-view').classList.add('active'); }
function formatDuration(s) { if (!s) return '0:00'; const h=Math.floor(s/3600),m=Math.floor((s%3600)/60),sec=s%60; return h>0?`${h}:${String(m).padStart(2,'0')}:${String(sec).padStart(2,'0')}`:`${m}:${String(sec).padStart(2,'0')}`; }
function formatDate(ts) { if (!ts) return ''; const d=new Date(ts*1000),n=Date.now(),diff=(n-d)/1000; if(diff<60) return 'just now'; if(diff<3600) return Math.floor(diff/60)+'m ago'; if(diff<86400) return Math.floor(diff/3600)+'h ago'; if(diff<2592000) return Math.floor(diff/86400)+'d ago'; if(diff<31536000) return Math.floor(diff/2592000)+'mo ago'; return Math.floor(diff/31536000)+'y ago'; }
function formatViews(c) { if (!c) return '0'; if(c>=1e6) return(c/1e6).toFixed(1)+'M'; if(c>=1e3) return(c/1e3).toFixed(1)+'K'; return c.toString(); }
function escapeHtml(str) { const d=document.createElement('div'); d.textContent=str||''; return d.innerHTML; }
function truncateText(t,l) { return t.length>l?t.substring(0,l)+'...':t; }
function debounce(fn,ms) { let t; return(...a)=>{clearTimeout(t);t=setTimeout(()=>fn(...a),ms);}; }

if (window.__TAURI_INTERNALS__) init().then(bindEvents);
else { const c=setInterval(()=>{if(window.__TAURI_INTERNALS__){clearInterval(c);init().then(bindEvents);}},100); }
