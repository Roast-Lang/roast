//! Statistics module for Roast
//!
//! Provides statistical functions like mean, median, stdev, variance.

/// Statistical functions
pub const ROAST_STATISTICS_SOURCE: &str = r##"
"""
Statistics module.

Provides statistical functions for data analysis including measures of
central tendency and dispersion.
"""

import math


def mean(data: list[float]) -> float:
    """Calculate arithmetic mean of data.
    
    Args:
        data: List of numeric values
        
    Returns:
        Arithmetic mean
    """
    if len(data) == 0:
        return 0.0
    
    total: float = 0.0
    for x in data:
        total = total + x
    
    return total / float(len(data))


def median(data: list[float]) -> float:
    """Calculate median of data.
    
    Args:
        data: List of numeric values
        
    Returns:
        Median value
    """
    if len(data) == 0:
        return 0.0
    
    # Sort the data
    sorted_data: list[float] = sorted(data)
    n: int = len(sorted_data)
    
    if n % 2 == 1:
        return sorted_data[n // 2]
    else:
        mid: int = n // 2
        return (sorted_data[mid - 1] + sorted_data[mid]) / 2.0


def mode(data: list[float]) -> float:
    """Calculate mode (most common value) of data.
    
    Args:
        data: List of numeric values
        
    Returns:
        Most common value (first if tie)
    """
    if len(data) == 0:
        return 0.0
    
    counts: dict[float, int] = {}
    for x in data:
        if x in counts:
            counts[x] = counts[x] + 1
        else:
            counts[x] = 1
    
    max_count: int = 0
    mode_val: float = data[0]
    
    for val in counts:
        if counts[val] > max_count:
            max_count = counts[val]
            mode_val = val
    
    return mode_val


def variance(data: list[float], population: bool = False) -> float:
    """Calculate variance of data.
    
    Args:
        data: List of numeric values
        population: If True, calculate population variance; otherwise sample variance
        
    Returns:
        Variance
    """
    n: int = len(data)
    if n == 0:
        return 0.0
    if n == 1:
        return 0.0
    
    m: float = mean(data)
    
    sum_sq: float = 0.0
    for x in data:
        diff: float = x - m
        sum_sq = sum_sq + diff * diff
    
    if population:
        return sum_sq / float(n)
    else:
        return sum_sq / float(n - 1)


def stdev(data: list[float], population: bool = False) -> float:
    """Calculate standard deviation of data.
    
    Args:
        data: List of numeric values
        population: If True, calculate population stdev; otherwise sample stdev
        
    Returns:
        Standard deviation
    """
    return math.sqrt(variance(data, population))


def pvariance(data: list[float]) -> float:
    """Calculate population variance.
    
    Args:
        data: List of numeric values
        
    Returns:
        Population variance
    """
    return variance(data, True)


def pstdev(data: list[float]) -> float:
    """Calculate population standard deviation.
    
    Args:
        data: List of numeric values
        
    Returns:
        Population standard deviation
    """
    return stdev(data, True)


def sum(data: list[float]) -> float:
    """Calculate sum of data.
    
    Args:
        data: List of numeric values
        
    Returns:
        Sum of all values
    """
    total: float = 0.0
    for x in data:
        total = total + x
    return total


def min(data: list[float]) -> float:
    """Find minimum value in data.
    
    Args:
        data: List of numeric values
        
    Returns:
        Minimum value
    """
    if len(data) == 0:
        return 0.0
    
    result: float = data[0]
    for x in data:
        if x < result:
            result = x
    return result


def max(data: list[float]) -> float:
    """Find maximum value in data.
    
    Args:
        data: List of numeric values
        
    Returns:
        Maximum value
    """
    if len(data) == 0:
        return 0.0
    
    result: float = data[0]
    for x in data:
        if x > result:
            result = x
    return result


def quantile(data: list[float], q: float) -> float:
    """Calculate quantile of data.
    
    Args:
        data: List of numeric values
        q: Quantile (0.0 to 1.0)
        
    Returns:
        Quantile value
    """
    if len(data) == 0:
        return 0.0
    
    if q <= 0.0:
        return min(data)
    if q >= 1.0:
        return max(data)
    
    sorted_data: list[float] = sorted(data)
    n: int = len(sorted_data)
    
    idx: float = q * float(n - 1)
    lower: int = int(idx)
    upper: int = lower + 1
    
    if upper >= n:
        return sorted_data[n - 1]
    
    frac: float = idx - float(lower)
    return sorted_data[lower] + frac * (sorted_data[upper] - sorted_data[lower])


def percentile(data: list[float], p: float) -> float:
    """Calculate percentile of data.
    
    Args:
        data: List of numeric values
        p: Percentile (0 to 100)
        
    Returns:
        Percentile value
    """
    return quantile(data, p / 100.0)


def correlation(x: list[float], y: list[float]) -> float:
    """Calculate Pearson correlation coefficient.
    
    Args:
        x: First list of values
        y: Second list of values
        
    Returns:
        Correlation coefficient (-1 to 1)
    """
    n: int = len(x)
    if n == 0 or len(y) != n:
        return 0.0
    
    mean_x: float = mean(x)
    mean_y: float = mean(y)
    
    sum_xy: float = 0.0
    sum_x2: float = 0.0
    sum_y2: float = 0.0
    
    for i in range(n):
        dx: float = x[i] - mean_x
        dy: float = y[i] - mean_y
        sum_xy = sum_xy + dx * dy
        sum_x2 = sum_x2 + dx * dx
        sum_y2 = sum_y2 + dy * dy
    
    if sum_x2 == 0.0 or sum_y2 == 0.0:
        return 0.0
    
    return sum_xy / math.sqrt(sum_x2 * sum_y2)
"##;
