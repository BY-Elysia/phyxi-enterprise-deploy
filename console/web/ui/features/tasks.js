import { EmptyState, h } from '../core/view.js';

function taskArea(job) {
  if (job.composition?.kind === 'gateways' || [
    'services_start', 'services_stop', 'management_start', 'management_stop',
    'gateway_start', 'gateway_stop', 'gateway_apply', 'certificate', 'publish',
  ].includes(job.action)) return 'gateway';
  if (['download', 'pull_image'].includes(job.action)) return 'resources';
  if (['prepare', 'repair_toolkit', 'verify_gpu'].includes(job.action)) return 'host';
  return 'model';
}

export function filterJobs(jobs, { area, state, query }, actionNames) {
  return jobs.filter(job =>
    (area === 'all' || taskArea(job) === area) &&
    (state === 'all' || job.status === state) &&
    ((job.composition?.name || '') + ' ' + (actionNames[job.action] || job.action)).toLowerCase().includes(query)
  );
}

export function TaskList({ jobs, active, actionNames, states, formatTime, onPick }) {
  if (!jobs.length) {
    return h(EmptyState, { title: '暂无任务', description: '下载资源或部署模型后，可在这里查看执行记录。' });
  }
  return jobs.map(job => h('button', {
    key: job.id,
    type: 'button',
    className: job.id === active ? 'job selected' : 'job',
    'aria-pressed': job.id === active,
    onClick: () => onPick(job),
  },
  h('span', null,
    h('strong', null, actionNames[job.action] || job.action),
    h('small', null, (job.composition?.name || '服务器任务') + ' · ' + formatTime(job.created)),
  ),
  h('span', { className: 'state ' + job.status }, states[job.status] || job.status)));
}
