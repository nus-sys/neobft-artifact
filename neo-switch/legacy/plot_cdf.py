#!/usr/bin/python3
'''
Usage: python3 plot_cdf.py FILENAME
'''

import sys
import os
import matplotlib
import matplotlib.pyplot as plt
import seaborn as sns 
import numpy as np

with open(sys.argv[1]) as f:
    data = f.readlines()
data = [ d.strip() for d in data ]
data = data[:1000000]
data = [ d.replace(':', '') for d in data ]
data = [ float(int(d, 16) / 1000) for d in data ]
data = sorted(data)


print("25.0th", int(np.percentile(data, 25)), "us")
print("50.0th", int(np.percentile(data, 50)), "us")
print("99.0th ", int(np.percentile(data, 99)), "us")
print("99.9th", int(np.percentile(data, 99.9)), "us")
print("99.99th", int(np.percentile(data, 99.99)), "us")
print("99.999th", int(np.percentile(data, 99.999)), "us")

y = 1. * np.arange(len(data)) / (len(data) - 1)
plt.plot(data, y)
plt.savefig('results/{}.png'.format(os.path.basename(sys.argv[1])))
# plt.show()
# sns.kdeplot(data = data, cumulative = True, label = "fpga")
# plt.legend()