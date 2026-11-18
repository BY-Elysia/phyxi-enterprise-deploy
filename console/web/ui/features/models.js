import { React, EmptyState, SectionHeading, h } from '../core/view.js';

const pageSize = 9;
const filters = [
  ['brand', '模型品牌', [['all', '全部品牌'], ['Qwen', 'Qwen'], ['GLM', 'GLM'], ['DeepSeek', 'DeepSeek']]],
  ['modality', '模型能力', [['all', '全部能力'], ['vision', '多模态 · 图像理解'], ['text', '文本生成']]],
  ['size', '参数规模', [['all', '全部规模'], ['small', '小型 · ≤ 8B'], ['medium', '中型 · > 8–32B'], ['large', '大型 · > 32B']]],
];

function sizeBand(parameters) {
  if (parameters <= 8) return 'small';
  if (parameters <= 32) return 'medium';
  return 'large';
}

function ModelCard({ model, recordId, formatBytes, onChoose, onOpen }) {
  const facts = [
    ['参数规模', model.parameters_b + 'B', model.active_parameters_b ? '激活 ' + model.active_parameters_b + 'B' : '稠密模型'],
    ['权重体积', formatBytes(model.weight_bytes), '官方文件快照'],
  ];
  return h('article', { className: 'model-card', 'data-preset': model.id },
    h('div', { className: 'model-card-head' },
      h('img', { src: model.logo, alt: model.brand, width: 38, height: 38 }),
      h('div', null,
        h('span', { className: 'model-brand-label' }, model.brand),
        h('h3', { title: model.name }, model.name),
      ),
    ),
    h('div', { className: 'model-tags' },
      [model.modality === 'vision' ? '多模态' : '文本生成', model.architecture, model.precision]
        .map((text, index) => h('span', { key: index }, text)),
    ),
    h('p', { className: 'model-card-description' }, model.description),
    h('dl', { className: 'model-card-facts' }, facts.map(([title, value, note]) =>
      h('div', { key: title }, h('dt', null, title), h('dd', null, value), h('small', null, note)),
    )),
    h('p', { className: 'model-card-backend', title: model.backend_note },
      '后端参考 · ' + model.backend + ' ' + model.backend_reference),
    h('div', { className: 'model-card-footer' },
      h('a', { href: `https://hf-mirror.com/${model.repo}`, target: '_blank', rel: 'noopener noreferrer', title: '许可：' + model.license }, '镜像站模型卡 ↗'),
      h('button', {
        type: 'button', className: recordId ? 'quiet' : 'model-choose',
        onClick: () => recordId ? onOpen(recordId) : onChoose(model),
      }, recordId ? '查看已添加' : '选择模型'),
    ),
  );
}

export default function Models({ presets, records, formatBytes, onOpen, onChoose, onCustom, onImport }) {
  const [query, setQuery] = React.useState({ search: '', brand: 'all', modality: 'all', size: 'all' });
  const [limit, setLimit] = React.useState(pageSize);
  const change = (key, value) => {
    setQuery(current => ({ ...current, [key]: value }));
    setLimit(pageSize);
  };
  const search = query.search.trim().toLowerCase();
  const entries = presets.filter(model =>
    (query.brand === 'all' || model.brand === query.brand) &&
    (query.modality === 'all' || model.modality === query.modality) &&
    (query.size === 'all' || sizeBand(model.parameters_b) === query.size) &&
    (!search || [model.name, model.brand, model.repo, model.description].join(' ').toLowerCase().includes(search))
  );
  const recordByRepo = new Map();
  for (const [id, record] of Object.entries(records)) {
    const repo = record.data.source?.repo;
    if (repo && !recordByRepo.has(repo)) recordByRepo.set(repo, id);
  }
  return h(React.Fragment, null,
    h(SectionHeading, { title: '选择适合你的模型', description: '官方来源预设，按需下载到服务器。' },
      h('button', { id: 'model-local', type: 'button', onClick: onImport }, '添加已有权重'),
      h('button', { id: 'model-custom', type: 'button', onClick: onCustom }, '＋ 自定义下载'),
    ),
    h('div', { className: 'model-filters' },
      h('label', { className: 'search-field' }, h('span', { className: 'sr-only' }, '搜索模型'),
        h('input', {
          id: 'model-search', type: 'search', placeholder: '搜索模型名称或系列…',
          value: query.search, onChange: event => change('search', event.target.value),
        }),
      ),
      filters.map(([key, label, options]) => h('label', { key },
        h('span', { className: 'sr-only' }, label),
        h('select', {
          id: 'model-' + key, value: query[key], onChange: event => change(key, event.target.value),
        }, options.map(([value, text]) => h('option', { key: value, value }, text))),
      )),
      h('span', { id: 'model-result-count', className: 'hint', role: 'status', 'aria-live': 'polite' },
        entries.length + ' 个模型'),
    ),
    h('div', { id: 'model-cards', className: 'model-card-grid' },
      entries.length ? entries.slice(0, limit).map(model => h(ModelCard, {
        key: model.id, model, recordId: recordByRepo.get(model.repo), formatBytes, onChoose, onOpen,
      })) : h(EmptyState, { title: '没有匹配的模型', description: '调整品牌、能力、规模或搜索关键词。' }),
    ),
    h('div', { className: 'model-more' },
      h('button', {
        id: 'model-show-more', type: 'button', hidden: limit >= entries.length, onClick: () => setLimit(limit + pageSize),
      }, '显示更多模型'),
    ),
    h('p', { className: 'model-catalog-note' },
      'B 为十亿参数；规模按官方标称，MoE 另列激活参数。权重体积来自官方文件快照，不等于运行显存需求。'),
  );
}
