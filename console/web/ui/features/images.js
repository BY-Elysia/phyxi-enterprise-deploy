import { React, SectionHeading, h } from '../core/view.js';

function EngineCard({ preset, records, onChoose, onOpen }) {
  const [selectedVersion, selectVersion] = React.useState(preset.default_version);
  const version = preset.versions.find(item => item.version === selectedVersion);
  const record = Object.entries(records).find(([, item]) =>
    item.data.engine === preset.engine && item.data.reference === version.reference);
  const platforms = version.platforms.map(platform => platform === 'amd64' ? 'x86_64' : platform);
  return h('article', { className: 'model-card engine-card', 'data-engine': preset.engine },
    h('div', { className: 'engine-wordmark ' + preset.engine }, preset.name),
    h('p', { className: 'engine-description' }, preset.description),
    h('p', { className: 'engine-repository' }, preset.repository),
    h('label', null, '镜像版本',
      h('select', {
        'aria-label': preset.name + ' 镜像版本',
        value: selectedVersion,
        onChange: event => selectVersion(event.target.value),
      }, preset.versions.map(item => h('option', { key: item.version, value: item.version },
        item.version + (item.version === preset.default_version ? ' · 默认参考' : ''),
      ))),
    ),
    h('p', { className: 'engine-version-meta' }, 'Linux · ' + platforms.join(' / ') + ' · ' + version.published),
    h('div', { className: 'model-card-footer' },
      h('a', { href: preset.homepage, target: '_blank', rel: 'noopener noreferrer' }, '官方文档 ↗'),
      h('button', {
        type: 'button', className: 'model-choose',
        onClick: () => record ? onOpen(record[0]) : onChoose(preset, version),
      }, record ? '查看已添加' : '选择版本'),
    ),
  );
}

export default function Images({ presets, records, onChoose, onOpen, onSettings, onDetect, onCustom }) {
  return h(React.Fragment, null,
    h(SectionHeading, { title: '选择推理后端', description: '选择版本，按需下载到服务器。' },
      h('button', { id: 'image-market-settings', type: 'button', onClick: onSettings }, '下载来源设置'),
      h('button', { id: 'image-local', type: 'button', onClick: onDetect }, '检测已有镜像'),
      h('button', { id: 'image-custom', type: 'button', onClick: onCustom }, '＋ 自定义镜像'),
    ),
    h('div', { id: 'image-cards', className: 'model-card-grid engine-card-grid' },
      presets.map(preset => h(EngineCard, { key: preset.id, preset, records, onChoose, onOpen })),
    ),
    h('p', { className: 'model-catalog-note' },
      '预设固定为官方镜像摘要，保存不会下载或重启服务。平台为 Linux NVIDIA GPU，需按所选版本核对驱动与模型支持。'),
  );
}
