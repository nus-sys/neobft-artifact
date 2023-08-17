#!/usr/bin/python3
'''
Usage: python3 plot_cdf_hmac.py RESULTS_FOLDER/
'''

import sys
import os
import matplotlib
import matplotlib.pyplot as plt
import seaborn as sns 
import numpy as np
from mpl_toolkits.axes_grid1.inset_locator import zoomed_inset_axes 
from mpl_toolkits.axes_grid1.inset_locator import mark_inset

matplotlib.rcParams['pdf.fonttype'] = 42
matplotlib.rcParams['font.family'] = 'sans-serif'
matplotlib.rcParams['font.sans-serif'] = ['Nimbus Roman']

ZOOM = True

# directory = 'results/'
directory = sys.argv[1]
files = os.listdir(directory)

lstyles = ['-', '--', '-.', '-']

xfiles = [ '25% load', '50% load', '99% load']
files = [ 'results_hmac25.log', 'results_hmac50.log', 'results_hmac99.log' ]
files = sorted(files)

fig, ax = plt.subplots()
res = []

for i, ff in enumerate(files):
    with open(os.path.join(directory, ff)) as f:
        data = f.readlines()
    data = [ d.strip() for d in data ]
    data = data[:1000000]
    data = [ d.replace(':', '') for d in data ]
    data = [ float(int(d, 16) / 1000) for d in data ]
    data = sorted(data)
    
    res.append(data)
    
    print("25.0th", int(np.percentile(data, 25)), "us")
    print("50.0th", int(np.percentile(data, 50)), "us")
    print("99.0th ", int(np.percentile(data, 99)), "us")
    print("99.9th", int(np.percentile(data, 99.9)), "us")
    print("99.99th", int(np.percentile(data, 99.99)), "us")
    print("99.999th", int(np.percentile(data, 99.999)), "us")
    
    # sns.kdeplot(data = data, cumulative = True, label = ff, )
    y = 1. * np.arange(len(data)) / (len(data) - 1)
    plt.plot(data, y, lstyles[i%4], label=xfiles[i])
    # plt.plot(data, y)
    
    
plt.tick_params(direction='in')
plt.xticks(fontsize=14)
plt.yticks(fontsize=14)
plt.xlabel("Latency (us)", fontsize=14)
plt.ylabel("CDF", fontsize=14)
plt.legend(fontsize=14)
plt.tight_layout()

if ZOOM:
    axins = zoomed_inset_axes(ax, 30, loc='center')
    axins.set_xlim([9.0, 9.6]) # Limit the region for zoom
    axins.set_ylim([0.99, 1.002])
    axins.set_yticks([0.99, 0.995, 1.0])
    axins.set_yticks([0.99, 1.0])
    
    axins.tick_params("x", labelsize=14)
    axins.tick_params("y", labelsize=14)
    # plt.xticks(visible=False)  # Not present ticks
    # plt.yticks(visible=False)
    ## draw a bbox of the region of the inset axes in the parent axes and
    ## connecting lines between the bbox and the inset axes area
    mark_inset(ax, axins, loc1=2, loc2=4, fc="none", ec="0.5")
    for i, data in enumerate(res):
        y = 1. * np.arange(len(data)) / (len(data) - 1)
        axins.plot(data, y, lstyles[i%4], label=xfiles[i])

# plt.savefig('results/cdf_hmac.png')
plt.savefig('results/cdf_hmac.pdf')

print(">> Done! See: results/cdf_hmac.pdf")
