class ChartManager {
    constructor() {
        this.charts = new Map();
        this.maxDataPoints = 50;
        this.colors = {
            primary: '#58a6ff',
            secondary: '#238636',
            warning: '#d29922',
            danger: '#da3633',
            grid: '#30363d',
            text: '#8b949e'
        };
    }

    initInferenceTimeChart(canvasId) {
        const canvas = document.getElementById(canvasId);
        if (!canvas) return;

        const ctx = canvas.getContext('2d');
        
        this.charts.set('inferenceTime', new Chart(ctx, {
            type: 'line',
            data: {
                labels: [],
                datasets: [{
                    label: 'Inference Time (ms)',
                    data: [],
                    borderColor: this.colors.primary,
                    backgroundColor: 'rgba(88, 166, 255, 0.1)',
                    fill: true,
                    tension: 0.4,
                    pointRadius: 2,
                    pointHoverRadius: 4
                }]
            },
            options: {
                responsive: true,
                maintainAspectRatio: false,
                plugins: {
                    legend: {
                        display: false
                    },
                    tooltip: {
                        backgroundColor: 'rgba(13, 17, 23, 0.9)',
                        titleColor: '#c9d1d9',
                        bodyColor: '#c9d1d9',
                        borderColor: '#30363d',
                        borderWidth: 1
                    }
                },
                scales: {
                    x: {
                        grid: {
                            color: this.colors.grid,
                            drawBorder: false
                        },
                        ticks: {
                            color: this.colors.text,
                            maxTicksLimit: 10
                        }
                    },
                    y: {
                        grid: {
                            color: this.colors.grid,
                            drawBorder: false
                        },
                        ticks: {
                            color: this.colors.text
                        },
                        beginAtZero: true
                    }
                }
            }
        }));
    }

    initCacheHitRateChart(canvasId) {
        const canvas = document.getElementById(canvasId);
        if (!canvas) return;

        const ctx = canvas.getContext('2d');
        
        this.charts.set('cacheHitRate', new Chart(ctx, {
            type: 'doughnut',
            data: {
                labels: ['Hits', 'Misses'],
                datasets: [{
                    data: [0, 0],
                    backgroundColor: [
                        this.colors.secondary,
                        this.colors.danger
                    ],
                    borderWidth: 0
                }]
            },
            options: {
                responsive: true,
                maintainAspectRatio: false,
                plugins: {
                    legend: {
                        position: 'bottom',
                        labels: {
                            color: '#c9d1d9',
                            padding: 20
                        }
                    },
                    tooltip: {
                        backgroundColor: 'rgba(13, 17, 23, 0.9)',
                        titleColor: '#c9d1d9',
                        bodyColor: '#c9d1d9',
                        borderColor: '#30363d',
                        borderWidth: 1
                    }
                },
                cutout: '60%'
            }
        }));
    }

    initNetworkBandwidthChart(canvasId) {
        const canvas = document.getElementById(canvasId);
        if (!canvas) return;

        const ctx = canvas.getContext('2d');
        
        this.charts.set('networkBandwidth', new Chart(ctx, {
            type: 'bar',
            data: {
                labels: [],
                datasets: [
                    {
                        label: 'Sent (MB)',
                        data: [],
                        backgroundColor: 'rgba(88, 166, 255, 0.7)',
                        borderColor: this.colors.primary,
                        borderWidth: 1
                    },
                    {
                        label: 'Received (MB)',
                        data: [],
                        backgroundColor: 'rgba(35, 134, 54, 0.7)',
                        borderColor: this.colors.secondary,
                        borderWidth: 1
                    }
                ]
            },
            options: {
                responsive: true,
                maintainAspectRatio: false,
                plugins: {
                    legend: {
                        display: true,
                        position: 'top',
                        labels: {
                            color: '#c9d1d9',
                            padding: 20
                        }
                    },
                    tooltip: {
                        backgroundColor: 'rgba(13, 17, 23, 0.9)',
                        titleColor: '#c9d1d9',
                        bodyColor: '#c9d1d9',
                        borderColor: '#30363d',
                        borderWidth: 1
                    }
                },
                scales: {
                    x: {
                        grid: {
                            display: false
                        },
                        ticks: {
                            color: this.colors.text,
                            maxTicksLimit: 10
                        }
                    },
                    y: {
                        grid: {
                            color: this.colors.grid,
                            drawBorder: false
                        },
                        ticks: {
                            color: this.colors.text
                        },
                        beginAtZero: true
                    }
                }
            }
        }));
    }

    initBlockchainGrowthChart(canvasId) {
        const canvas = document.getElementById(canvasId);
        if (!canvas) return;

        const ctx = canvas.getContext('2d');
        
        this.charts.set('blockchainGrowth', new Chart(ctx, {
            type: 'line',
            data: {
                labels: [],
                datasets: [
                    {
                        label: 'Blocks',
                        data: [],
                        borderColor: this.colors.primary,
                        backgroundColor: 'rgba(88, 166, 255, 0.1)',
                        fill: true,
                        tension: 0.4,
                        yAxisID: 'y'
                    },
                    {
                        label: 'Transactions',
                        data: [],
                        borderColor: this.colors.secondary,
                        backgroundColor: 'rgba(35, 134, 54, 0.1)',
                        fill: true,
                        tension: 0.4,
                        yAxisID: 'y1'
                    }
                ]
            },
            options: {
                responsive: true,
                maintainAspectRatio: false,
                interaction: {
                    mode: 'index',
                    intersect: false
                },
                plugins: {
                    legend: {
                        display: true,
                        position: 'top',
                        labels: {
                            color: '#c9d1d9',
                            padding: 20
                        }
                    },
                    tooltip: {
                        backgroundColor: 'rgba(13, 17, 23, 0.9)',
                        titleColor: '#c9d1d9',
                        bodyColor: '#c9d1d9',
                        borderColor: '#30363d',
                        borderWidth: 1
                    }
                },
                scales: {
                    x: {
                        grid: {
                            color: this.colors.grid,
                            drawBorder: false
                        },
                        ticks: {
                            color: this.colors.text,
                            maxTicksLimit: 10
                        }
                    },
                    y: {
                        type: 'linear',
                        display: true,
                        position: 'left',
                        grid: {
                            color: this.colors.grid,
                            drawBorder: false
                        },
                        ticks: {
                            color: this.colors.text
                        }
                    },
                    y1: {
                        type: 'linear',
                        display: true,
                        position: 'right',
                        grid: {
                            drawOnChartArea: false
                        },
                        ticks: {
                            color: this.colors.text
                        }
                    }
                }
            }
        }));
    }

    initAllCharts() {
        this.initInferenceTimeChart('inferenceTimeChart');
        this.initCacheHitRateChart('cacheHitRateChart');
        this.initNetworkBandwidthChart('networkBandwidthChart');
        this.initBlockchainGrowthChart('blockchainGrowthChart');
    }

    updateInferenceTimeChart(data) {
        const chart = this.charts.get('inferenceTime');
        if (!chart) return;

        const timestamp = new Date().toLocaleTimeString();
        
        chart.data.labels.push(timestamp);
        chart.data.datasets[0].data.push(data);

        if (chart.data.labels.length > this.maxDataPoints) {
            chart.data.labels.shift();
            chart.data.datasets[0].data.shift();
        }

        chart.update('none');
    }

    updateCacheHitRateChart(hits, misses) {
        const chart = this.charts.get('cacheHitRate');
        if (!chart) return;

        chart.data.datasets[0].data = [hits, misses];
        chart.update('none');
    }

    updateNetworkBandwidthChart(sent, received) {
        const chart = this.charts.get('networkBandwidth');
        if (!chart) return;

        const timestamp = new Date().toLocaleTimeString();
        
        chart.data.labels.push(timestamp);
        chart.data.datasets[0].data.push(sent);
        chart.data.datasets[1].data.push(received);

        if (chart.data.labels.length > this.maxDataPoints) {
            chart.data.labels.shift();
            chart.data.datasets[0].data.shift();
            chart.data.datasets[1].data.shift();
        }

        chart.update('none');
    }

    updateBlockchainGrowthChart(blocks, transactions) {
        const chart = this.charts.get('blockchainGrowth');
        if (!chart) return;

        const timestamp = new Date().toLocaleTimeString();
        
        chart.data.labels.push(timestamp);
        chart.data.datasets[0].data.push(blocks);
        chart.data.datasets[1].data.push(transactions);

        if (chart.data.labels.length > this.maxDataPoints) {
            chart.data.labels.shift();
            chart.data.datasets[0].data.shift();
            chart.data.datasets[1].data.shift();
        }

        chart.update('none');
    }

    updateCharts(metricsData) {
        if (metricsData.ai) {
            if (metricsData.ai.inferenceTime !== undefined) {
                this.updateInferenceTimeChart(metricsData.ai.inferenceTime);
            }
            
            if (metricsData.ai.cacheHits !== undefined && metricsData.ai.cacheMisses !== undefined) {
                this.updateCacheHitRateChart(metricsData.ai.cacheHits, metricsData.ai.cacheMisses);
            }
        }

        if (metricsData.network) {
            if (metricsData.network.bytesSent !== undefined && metricsData.network.bytesReceived !== undefined) {
                this.updateNetworkBandwidthChart(
                    metricsData.network.bytesSent / 1024 / 1024,
                    metricsData.network.bytesReceived / 1024 / 1024
                );
            }
        }

        if (metricsData.blockchain) {
            if (metricsData.blockchain.totalBlocks !== undefined && metricsData.blockchain.totalTransactions !== undefined) {
                this.updateBlockchainGrowthChart(
                    metricsData.blockchain.totalBlocks,
                    metricsData.blockchain.totalTransactions
                );
            }
        }
    }

    destroyChart(chartName) {
        const chart = this.charts.get(chartName);
        if (chart) {
            chart.destroy();
            this.charts.delete(chartName);
        }
    }

    destroyAllCharts() {
        this.charts.forEach((chart, name) => {
            chart.destroy();
        });
        this.charts.clear();
    }

    resizeCharts() {
        this.charts.forEach((chart) => {
            chart.resize();
        });
    }
}

if (typeof module !== 'undefined' && module.exports) {
    module.exports = ChartManager;
}
