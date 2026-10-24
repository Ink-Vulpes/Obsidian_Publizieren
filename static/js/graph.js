/* graph.js — D3 local graph for the right sidebar */

document.addEventListener('DOMContentLoaded', async function () {
  const container = document.getElementById('local-graph');
  if (!container || typeof d3 === 'undefined') return;

  const currentSlug = window.__SLUG__ || '';
  if (!currentSlug) return;

  let graphData;
  try {
    const res = await fetch('/api/graph');
    graphData = await res.json();
  } catch {
    return;
  }

  const { nodes, links } = graphData;
  if (!nodes || nodes.length === 0) return;

  // Filter to local neighbourhood (current + adjacent)
  const adjacentIds = new Set([currentSlug]);
  links.forEach(l => {
    if (l.source === currentSlug) adjacentIds.add(l.target);
    if (l.target === currentSlug) adjacentIds.add(l.source);
  });

  const localNodes = nodes.filter(n => adjacentIds.has(n.id));
  const localLinks = links.filter(l => adjacentIds.has(l.source) && adjacentIds.has(l.target));

  if (localNodes.length === 0) return;

  const w = container.clientWidth || 200;
  const h = container.clientHeight || 180;

  const svg = d3.select(container)
    .append('svg')
    .attr('width', w)
    .attr('height', h);

  const g = svg.append('g');

  // Zoom
  svg.call(d3.zoom()
    .scaleExtent([0.3, 4])
    .on('zoom', e => g.attr('transform', e.transform))
  );

  const linkSel = g.append('g').attr('class', 'graph-link')
    .selectAll('line')
    .data(localLinks)
    .join('line');

  const nodeSel = g.append('g')
    .selectAll('.graph-node')
    .data(localNodes)
    .join('g')
    .attr('class', d => 'graph-node' + (d.id === currentSlug ? ' current' : ''))
    .style('cursor', 'pointer')
    .on('click', (_, d) => { window.location.href = '/note/' + d.id; });

  nodeSel.append('circle')
    .attr('r', d => d.id === currentSlug ? 7 : Math.min(5 + d.link_count, 8));

  nodeSel.append('text')
    .attr('dy', 14)
    .text(d => d.title.length > 14 ? d.title.slice(0, 13) + '…' : d.title);

  nodeSel.append('title').text(d => d.title);

  const sim = d3.forceSimulation(localNodes)
    .force('link', d3.forceLink(localLinks)
      .id(d => d.id)
      .distance(55)
      .strength(0.8)
    )
    .force('charge', d3.forceManyBody().strength(-80))
    .force('center', d3.forceCenter(w / 2, h / 2))
    .force('collision', d3.forceCollide(16));

  sim.on('tick', () => {
    linkSel
      .attr('x1', d => d.source.x)
      .attr('y1', d => d.source.y)
      .attr('x2', d => d.target.x)
      .attr('y2', d => d.target.y);
    nodeSel.attr('transform', d => `translate(${d.x},${d.y})`);
  });

  // Drag
  nodeSel.call(
    d3.drag()
      .on('start', (event, d) => {
        if (!event.active) sim.alphaTarget(0.3).restart();
        d.fx = d.x; d.fy = d.y;
      })
      .on('drag', (event, d) => { d.fx = event.x; d.fy = event.y; })
      .on('end', (event, d) => {
        if (!event.active) sim.alphaTarget(0);
        d.fx = null; d.fy = null;
      })
  );
});
