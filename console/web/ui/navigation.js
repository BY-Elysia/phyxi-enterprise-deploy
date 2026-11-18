import { h } from './core/view.js';

const groups = {
  deployments: '模型服务',
  models: '资源与配置',
  gateways: '接入',
  environment: '运维与审计',
};

export function Navigation({ titles, active, paths, onNavigate }) {
  return Object.entries(titles).flatMap(([view, title]) => {
    const selected = view === active;
    const link = h('button', {
      key: view,
      type: 'button',
      'data-view': view,
      className: selected ? 'nav-item active' : 'nav-item',
      'aria-current': selected ? 'page' : undefined,
      onClick: () => onNavigate(view),
    },
    h('svg', { viewBox: '0 0 24 24', 'aria-hidden': true }, h('path', { d: paths[view] || paths.tasks })),
    title);
    return groups[view]
      ? [h('p', { key: groups[view], className: 'nav-label' }, groups[view]), link]
      : [link];
  });
}
