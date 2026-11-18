'use strict';

export function mountServerMonitor({api, onAction, formatBytes, isActive, isConnected}) {
 const $ = id => document.getElementById(id);
 let monitorData={samples:[],metadata:{}},monitorLoading=false,monitorTab='metrics';
 let gpuMonitorData={samples:[],bucket_seconds:10},gpuMonitorRequest;
 const monitorDialog=$('monitor-chart-dialog');
 let enlargedChart;
 for(const tab of document.querySelectorAll('[data-monitor-tab]'))tab.onclick=onAction(async()=>{
  monitorTab=tab.dataset.monitorTab;
  document.querySelector('.monitor-toolbar').hidden=monitorTab==='software';
  for(const item of document.querySelectorAll('[data-monitor-tab]')){
   const selected=item===tab;item.setAttribute('aria-pressed',String(selected));
   $('monitor-'+item.dataset.monitorTab).hidden=!selected;
  }
  if(monitorTab==='metrics')renderMonitor();
  if(monitorTab==='gpus')renderGpuMonitor();
  if(monitorTab==='software')$('monitor-freshness').hidden=true;
  if(isConnected())await loadMonitor();
 });
 $('monitor-range').onchange=onAction(async()=>{
  if(monitorTab==='gpus')await loadGpuMonitor();
  else renderMonitor();
 });
 $('monitor-auto').onchange=()=>{if($('monitor-auto').checked)loadMonitor();};
 async function loadMonitor(){
  if(monitorTab==='gpus')return loadGpuMonitor();
  if(monitorTab==='software')return;
  if(monitorLoading)return;monitorLoading=true;
  try{monitorData=await api('/monitor');$('monitor-error').hidden=true;renderMonitor();}
  catch(error){$('monitor-error').hidden=false;$('monitor-error').textContent='监控数据读取失败，图表保留上次采样。'+error.message;if(monitorTab==='metrics')monitorFreshness(null,false,true);renderEnlargedChart();}
  finally{monitorLoading=false;}
 }
 async function loadGpuMonitor(){
  gpuMonitorRequest?.abort();
  const request=new AbortController();gpuMonitorRequest=request;
  const range=$('monitor-range').value;
  try{
   const data=await api('/monitor/gpus?seconds='+range,{signal:request.signal});
   if(request.signal.aborted)return;
   gpuMonitorData=data;$('gpu-monitor-error').hidden=true;
   if(monitorTab==='gpus')renderGpuMonitor();
  }catch(error){
   if(request.signal.aborted)return;
   $('gpu-monitor-error').textContent='GPU 采样读取失败，图表保留上次采样。'+error.message;
   $('gpu-monitor-error').hidden=false;
   if(monitorTab==='gpus')monitorFreshness(null,false,true);
   renderEnlargedChart();
  }
 }
 function monitorSvg(tag,attributes={}){const el=document.createElementNS('http://www.w3.org/2000/svg',tag);for(const [k,v]of Object.entries(attributes))el.setAttribute(k,v);return el;}
 function monitorNumber(value,unit){return Number.isFinite(value)?(unit==='%'?value.toFixed(1):(value/1024**2).toFixed(2)):'—';}
 function monitorFreshness(latest,stale,failed){
  const timestamp=latest&&new Date(latest.unix*1000).toLocaleTimeString('zh-CN',{hour12:false});
  $('monitor-freshness').textContent=failed?'连接失败':timestamp?(stale?'采样已过期 · ':'更新于 ')+timestamp:'等待首次采样';
  $('monitor-freshness').hidden=!failed&&Boolean(latest)&&!stale;
  $('inspect').title=timestamp?'上次采样 '+timestamp:'刷新监控与依赖';
 }
 function chartPreview(chart){
  const trigger=document.createElement('button');trigger.type='button';trigger.className='monitor-chart-trigger';
  trigger.dataset.monitorChart=chart.key;
  trigger.setAttribute('aria-label','放大 '+chart.title);
  trigger.setAttribute('aria-haspopup','dialog');
  const expand=document.createElement('span');expand.className='monitor-expand';expand.textContent='⤢';expand.setAttribute('aria-hidden','true');
  const svg=monitorSvg('svg',{viewBox:'0 0 540 168',class:'monitor-chart','aria-hidden':'true'});
  trigger.append(svg,expand);
  trigger.onclick=()=>{
   enlargedChart=chart;
   monitorDialog.showModal();
   renderEnlargedChart();
  };
  return {trigger,svg,chart};
 }
 function renderChartPreview({svg,chart}){
  drawMonitorChart(svg,chart);
  if(monitorDialog.open&&enlargedChart?.key===chart.key){
   enlargedChart=chart;
   renderEnlargedChart();
  }
 }
 function renderEnlargedChart(){
  if(!monitorDialog.open||!enlargedChart)return;
  const chart=enlargedChart;
  $('monitor-chart-title').textContent=chart.title;
  const status=$(chart.gpu?'gpu-monitor-error':'monitor-error');
  $('monitor-chart-detail').textContent=[
   $('monitor-range').selectedOptions[0].textContent,
   chart.detail,
   !$('monitor-auto').checked?'已暂停自动刷新':'',
   !status.hidden?'读取失败 · 保留上次采样':chart.stale?'采样已过期':'',
  ].filter(Boolean).join(' · ');
  const values=$('monitor-chart-values');values.replaceChildren();
  chart.series.forEach(([key,label],index)=>{
   const item=document.createElement('span');item.className=index?'secondary':'';
   const number=monitorNumber(chart.stale?null:chart.latest?.[key],chart.unit);
   item.textContent=label+' '+(number==='—'?number:number+(chart.unit==='%'?'%':' '+chart.unit));
   values.append(item);
  });
  drawMonitorChart($('monitor-chart-plot'),chart,true);
 }
 monitorDialog.addEventListener('close',()=>{
  if(monitorDialog.open)return;
  const key=enlargedChart?.key;
  enlargedChart=null;
  if(key)document.querySelector('[data-monitor-chart="'+CSS.escape(key)+'"]')?.focus({preventScroll:true});
 });
 new ResizeObserver(renderEnlargedChart).observe($('monitor-chart-plot'));
 window.addEventListener('hashchange',()=>{if(!isActive()&&monitorDialog.open)monitorDialog.close();});

 function renderMonitor(){
  const samples=monitorData.samples||[],meta=monitorData.metadata||{},m=meta.metrics||{},latest=samples.at(-1),now=Date.now()/1000,stale=latest&&now-latest.unix>45;
  $('monitor-host-name').textContent=meta.scope==='host'?'服务器宿主机':meta.scope==='container'?'容器采样环境':'本地主机';
  $('monitor-host-spec').textContent=[meta.platform||'系统待检测',m.logical_cpus?m.logical_cpus+' 个逻辑核心':null,m.memory?formatBytes(m.memory.total_bytes)+' 内存':null].filter(Boolean).join(' · ');
  if(monitorTab==='metrics')monitorFreshness(latest,stale,!$('monitor-error').hidden);
  const range=Number($('monitor-range').value),start=now-range,visible=samples.filter(s=>s.unix>=start&&s.unix<=now);
  const configs=[['CPU 使用率','%', [['cpu_percent','CPU']],meta.platform==='macos'?'最近 1 秒采样窗口':'相邻采样间隔内平均'],['内存使用率','%',[['memory_percent','已使用']],m.memory?(m.memory.estimated?'估算 · ':'')+formatBytes(m.memory.used_bytes)+' / '+formatBytes(m.memory.total_bytes):'内存数据暂不可用'],['磁盘 I/O','MiB/s',[['disk_read_bps','读取'],['disk_write_bps','写入']],'全部可见物理磁盘合计'],['网络吞吐','MiB/s',[['network_receive_bps','接收'],['network_send_bps','发送']],'全部非回环接口合计']];
  const root=$('monitor-charts');root.replaceChildren();
  for(const [title,unit,series,note]of configs){
   const card=document.createElement('article');card.className='monitor-chart-card';card.dataset.metric=series[0][0];const h=document.createElement('h3');h.textContent=title+(series[0][0]==='memory_percent'&&m.memory?.estimated?'（估算）':'');h.title=note;h.setAttribute('aria-label',h.textContent+'，'+note);
   const reading=document.createElement('div');reading.className='monitor-reading';const number=document.createElement('strong');number.textContent=monitorNumber(stale?null:latest?.[series[0][0]],unit);const suffix=document.createElement('span');suffix.textContent=unit;reading.title=series[0][1]+' · '+note;reading.append(number,suffix);card.append(h,reading);
   const preview=chartPreview({key:series[0][0],title:h.textContent,detail:note,points:visible,latest,stale,series,unit,start,range,maxGap:45});
   root.append(card);card.append(preview.trigger);const legend=document.createElement('div');legend.className='monitor-legend';series.forEach(([key,label],index)=>{const item=document.createElement('span');item.className=index?'secondary':'';item.textContent=label;item.title=monitorNumber(stale?null:latest?.[key],unit)+' '+unit;legend.append(item);});if(series.length>1)card.append(legend);
   renderChartPreview(preview);
  }
  const disk=$('monitor-disk');disk.replaceChildren();const heading=document.createElement('div');const name=document.createElement('h3');name.textContent='磁盘容量';heading.title=m.disk_path||'数据目录所在磁盘';heading.append(name);disk.append(heading);
  if(m.disk){const usedPercent=m.disk.total_bytes>0?m.disk.used_bytes/m.disk.total_bytes*100:0;const meter=document.createElement('div');meter.className='monitor-disk-meter';const label=document.createElement('span');label.textContent=formatBytes(m.disk.used_bytes)+' / '+formatBytes(m.disk.total_bytes);const progress=document.createElement('progress');progress.max=100;progress.value=usedPercent;progress.setAttribute('aria-label','磁盘已用比例');meter.append(label,progress);const free=document.createElement('span');free.textContent='可用 '+formatBytes(m.disk.available_bytes);disk.append(meter,free);}else{const empty=document.createElement('span');empty.textContent='磁盘容量暂不可用';disk.append(empty);}
 }
 function drawMonitorChart(svg,chart,expanded=false){
   const {points:visible,series,unit,start,range,maxGap}=chart;
   const width=svg.clientWidth||540,chartWidth=expanded?width:540;
   const chartHeight=Math.max(64,(svg.clientHeight||160)*chartWidth/width),plotBottom=chartHeight-22,plotHeight=plotBottom-10;
   const left=42,right=chartWidth-22,plotWidth=right-left,xAt=unix=>left+(unix-start)/range*plotWidth;
   svg.replaceChildren();svg.setAttribute('viewBox',`0 0 ${chartWidth} ${chartHeight}`);
   const values=visible.flatMap(s=>series.map(([key])=>s[key])).filter(Number.isFinite),max=unit==='%'?100:Math.max(1024**2,...values)*1.15;
   for(let i=0;i<4;i++){const y=10+i*plotHeight/3;svg.append(monitorSvg('line',{x1:left,y1:y,x2:right,y2:y,stroke:'#edf0f4'}));const label=monitorSvg('text',{x:left-8,y:y+3,'text-anchor':'end'});label.textContent=unit==='%'?String(Math.round(100-i*100/3)):((max-i*max/3)/1024**2).toFixed(1);svg.append(label);}
   const timeSteps=expanded&&chartWidth<450?3:5;
   for(let i=0;i<=timeSteps;i++){const unix=start+range*i/timeSteps,label=monitorSvg('text',{x:xAt(unix),y:chartHeight-4,'text-anchor':'middle'});label.textContent=new Date(unix*1000).toLocaleTimeString('zh-CN',{hour:'2-digit',minute:'2-digit',hour12:false});svg.append(label);}
   series.forEach(([key],index)=>{
    let path='',previous=null;for(const sample of visible){const value=sample[key];if(!Number.isFinite(value)){previous=null;continue;}const x=xAt(sample.unix),y=plotBottom-value/max*plotHeight;path+=(previous&&sample.unix-previous<=maxGap?'L':'M')+x.toFixed(2)+','+y.toFixed(2);previous=sample.unix;}
    svg.append(monitorSvg('path',{d:path,fill:'none',stroke:index?'var(--monitor-series-secondary)':'var(--monitor-series-primary)','stroke-dasharray':index?'5 3':'none','stroke-width':1.8,'stroke-linejoin':'round'}));
    const lastPoint=visible.findLast(sample=>Number.isFinite(sample[key]));if(lastPoint)svg.append(monitorSvg('circle',{cx:xAt(lastPoint.unix),cy:plotBottom-lastPoint[key]/max*plotHeight,r:2,fill:index?'var(--monitor-series-secondary)':'var(--monitor-series-primary)'}));
   });
   if(!values.length){const label=monitorSvg('text',{x:(left+right)/2,y:10+plotHeight/2,'text-anchor':'middle',class:'monitor-empty'});label.textContent=visible.length?'该指标暂不可用 / 等待下一次采样':'此时间范围暂无采样';svg.append(label);}
 }

 function gpuMemoryPercent(gpu){
  return gpu?.total_mib>0&&gpu.used_mib>=0&&gpu.used_mib<=gpu.total_mib?gpu.used_mib/gpu.total_mib*100:null;
 }
 function renderGpuMonitor(){
  const samples=gpuMonitorData.samples,latest=samples.at(-1),now=Date.now()/1000,stale=latest&&now-latest.unix>45;
  const inventory=samples.findLast(sample=>Array.isArray(sample.gpus))?.gpus||[];
  const root=$('host-gpus'),empty=$('gpu-monitor-empty');
  root.replaceChildren();empty.hidden=inventory.length>0;
  empty.textContent=latest?'暂无 NVIDIA GPU 采样，请检查设备和驱动。':'等待首次 GPU 采样…';
  monitorFreshness(latest,stale,!$('gpu-monitor-error').hidden);
  const range=Number($('monitor-range').value),start=now-range;
  const visible=samples.filter(sample=>sample.unix>=start&&sample.unix<=now);
  const series=[['utilization','GPU 利用率'],['memory','显存占用率']];
  const charts=[];
  for(const device of inventory){
   const current=!stale&&latest?.gpus?.find(gpu=>gpu.uuid===device.uuid);
   const card=document.createElement('article');card.className='monitor-chart-card gpu-monitor-card';card.dataset.gpuUuid=device.uuid;
   const title=document.createElement('h3');title.textContent=(Number.isInteger(device.index)?'GPU '+device.index+' · ':'')+device.name;
   const uuid=document.createElement('small');uuid.className='gpu-monitor-uuid';uuid.textContent=device.uuid;
   const legend=document.createElement('div');legend.className='monitor-legend gpu-monitor-values';
   const utilization=monitorNumber(current?.utilization_percent,'%'),memory=monitorNumber(gpuMemoryPercent(current),'%');
   for(const [label,value,secondary] of [
    ['GPU 利用率',utilization==='—'?utilization:utilization+'%',false],
    ['显存占用',memory==='—'?memory:`${(current.used_mib/1024).toFixed(1)} / ${(current.total_mib/1024).toFixed(1)} GiB · ${memory}%`,true],
   ]){
    const item=document.createElement('span');item.className=secondary?'secondary':'';item.append(label+' ');
    const number=document.createElement('strong');number.textContent=value;item.append(number);legend.append(item);
   }
   const points=visible.map(sample=>{
    const gpu=sample.gpus?.find(row=>row.uuid===device.uuid);
    return {unix:sample.unix,utilization:gpu?.utilization_percent,memory:gpuMemoryPercent(gpu)};
   });
   const preview=chartPreview({
    key:device.uuid,title:title.textContent,detail:device.uuid,gpu:true,points,
    latest:{utilization:current?.utilization_percent,memory:gpuMemoryPercent(current)},
    stale,series,unit:'%',start,range,maxGap:Math.max(45,gpuMonitorData.bucket_seconds*2),
   });
   root.append(card);card.append(title,uuid,legend,preview.trigger);
   charts.push(preview);
  }
  for(const preview of charts)renderChartPreview(preview);
 }

 let monitorChartSize='';
 new ResizeObserver(entries=>{const {width,height}=entries[0].contentRect;const size=Math.round(width)+'x'+Math.round(height);if(width>0&&height>0&&size!==monitorChartSize){monitorChartSize=size;requestAnimationFrame(()=>{if(isActive()&&monitorTab==='metrics')renderMonitor();});}}).observe($('monitor-charts'));
 let gpuMonitorWidth=0;
 new ResizeObserver(([entry])=>{
  const width=Math.round(entry.contentRect.width);
  if(width&&width!==gpuMonitorWidth){gpuMonitorWidth=width;requestAnimationFrame(()=>{if(isActive()&&monitorTab==='gpus')renderGpuMonitor();});}
 }).observe($('host-gpus'));

 const timerInterval = 15000;
 setInterval(()=>{
  if(isConnected()&&!document.hidden&&isActive()&&$('monitor-auto').checked)loadMonitor();
 },timerInterval);
 return {load:loadMonitor};
}
