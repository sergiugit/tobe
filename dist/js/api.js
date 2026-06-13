// Tauri v2 IPC bridge - available natively in webview
const invoke = window.__TAURI_INTERNALS__.invoke;

export async function getSubscriptions() {
  return await invoke('get_subscriptions');
}

export async function addSubscription(channel) {
  return await invoke('add_subscription', { channel });
}

export async function removeSubscription(channelId) {
  return await invoke('remove_subscription', { channelId });
}

export async function updateSubscriptionAvatar(channelId, channelAvatar) {
  return await invoke('update_subscription_avatar', { channelId, channelAvatar });
}

export async function getChannelVideos(channelId, page, invidiousInstance) {
  return await invoke('get_channel_videos', { channelId, page, invidiousInstance });
}

export async function getChannelLive(channelId, invidiousInstance) {
  return await invoke('get_channel_live', { channelId, invidiousInstance });
}

export async function getVideoUrl(videoId, formatId) {
  const args = { videoId };
  if (formatId) {
    args.formatId = formatId;
  }
  return await invoke('get_video_url', args);
}

export async function getVideoFormats(videoId) {
  return await invoke('get_video_formats', { videoId });
}

export async function getThumbnails(thumbnails) {
  return await invoke('get_thumbnails', { thumbnails });
}

export async function getComments(videoId, invidiousInstance) {
  return await invoke('get_comments', { videoId, invidiousInstance });
}

export async function getChannelAvatar(channelId) {
  return await invoke('get_channel_avatar', { channelId });
}

export async function getSubscribedFeed(subscriptions, page, invidiousInstance) {
  return await invoke('get_subscribed_feed', { subscriptions, page, invidiousInstance });
}

export async function getVideo(videoId, invidiousInstance) {
  return await invoke('get_video', { videoId, invidiousInstance });
}

export async function getSuggestions(query, invidiousInstance) {
  return await invoke('get_suggestions', { query, invidiousInstance });
}

export async function searchChannels(query, invidiousInstance) {
  return await invoke('search_channels', { query, invidiousInstance });
}
