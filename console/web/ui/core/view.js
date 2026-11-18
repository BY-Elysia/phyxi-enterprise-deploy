import { React, createRoot } from '../vendor/react-19.3.1.js';

export { React };
export const h = React.createElement;

export function EmptyState({ title, description }) {
  return h('div', { className: 'empty-state' },
    h('span', { className: 'empty-icon', 'aria-hidden': true }, '◇'),
    h('h3', null, title),
    h('p', null, description),
  );
}

class ViewError extends React.Component {
  state = { failed: false };

  static getDerivedStateFromError() {
    return { failed: true };
  }

  render() {
    if (!this.state.failed) return this.props.children;
    return h('div', { className: 'empty-state', role: 'alert' },
      h('p', null, '页面加载失败，请刷新后重试。'),
      h('button', { type: 'button', onClick: () => location.reload() }, '刷新页面'),
    );
  }
}

export function createView(container, Component) {
  let root;
  return props => {
    root ??= createRoot(container);
    root.render(h(ViewError, null,
      h(React.Suspense, { fallback: h('p', { className: 'hint', role: 'status' }, '正在加载…') },
        h(Component, props),
      ),
    ));
  };
}

export function SectionHeading({ title, description, children }) {
  return h('div', { className: 'model-market-heading' },
    h('div', null, h('h2', null, title), h('p', null, description)),
    h('div', { className: 'actions' }, children),
  );
}
