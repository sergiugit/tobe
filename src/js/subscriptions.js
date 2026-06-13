// Subscription management UI helpers
// (main logic is in app.js)

export function renderSubscriptionItem(sub) {
  // If avatar looks like a channel URL (not an image), treat as empty
  const hasValidAvatar = sub.channel_avatar && !sub.channel_avatar.includes('/channel/');
  const avatarHtml = hasValidAvatar
    ? `<img class="subscription-avatar" src="${sub.channel_avatar}" alt="" onerror="this.style.display='none'">`
    : `<div class="subscription-avatar avatar-letter">${(sub.channel_name || '?')[0].toUpperCase()}</div>`;

  return `
    <div class="subscription-item" data-id="${sub.channel_id}">
      ${avatarHtml}
      <span class="subscription-name">${sub.channel_name}</span>
    </div>
  `;
}
