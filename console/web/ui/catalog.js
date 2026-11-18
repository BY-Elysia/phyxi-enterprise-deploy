import { React, EmptyState, h } from './core/view.js';

function ResourceCard({ id, record, kind, label, icon, facts, state, onOpen, onEdit }) {
  const gateway = kind === 'gateways';
  return h('article', { className: 'resource-card', 'data-gateway-id': gateway ? id : undefined },
    h('div', { className: 'resource-head' },
      h('div', { className: 'resource-identity' },
        h('span', { className: 'resource-icon', 'aria-hidden': true },
          h('svg', { viewBox: '0 0 24 24' }, h('path', { d: icon })),
        ),
        h('div', null,
          h('h2', null, h('button', { type: 'button', className: 'link-button record-name', onClick: onOpen }, record.name)),
          h('p', { className: 'resource-caption' }, label + ' · 配置 r' + record.revision),
        ),
      ),
      h('div', { className: 'resource-actions' },
        state && h('span', {
          className: 'deployment-state resource-state', 'data-id': id, 'data-state': state.value,
        }, state.label),
        gateway && h('div', { className: 'gateway-list-state' }, '检查状态中…'),
        h('button', { type: 'button', className: 'quiet', onClick: onEdit }, '编辑配置'),
        h('button', { type: 'button', className: 'resource-open', onClick: onOpen },
          kind === 'deployments' ? '查看部署' : '查看详情'),
      ),
    ),
    h('dl', { className: 'resource-facts', style: { '--fact-count': facts.length } },
      facts.map(([label, value]) => {
        const text = value == null || value === '' ? '未设置' : String(value);
        return h('div', { key: label }, h('dt', null, label), h('dd', { title: text }, text));
      }),
    ),
    // Gateway readiness owns the children of this integration point.
    gateway && h('div', { className: 'gateway-list-controls' }),
  );
}

export function Catalog({ kind, label, icon, entries, filterActive, onOpen, onEdit, onCommit }) {
  React.useEffect(() => {
    if (kind === 'gateways') onCommit();
  }, [kind, entries, onCommit]);

  if (!entries.length) {
    return h(EmptyState, {
      title: filterActive ? '没有匹配的配置' : '尚未创建' + label,
      description: filterActive ? '尝试其他名称或清空搜索条件。' : '新建' + label + '，开始配置。',
    });
  }
  return entries.map(({ id, record, facts, state }) => h(ResourceCard, {
    key: kind + '/' + id, id, record, kind, label, icon, facts, state,
    onOpen: () => onOpen(id), onEdit: () => onEdit(id),
  }));
}
